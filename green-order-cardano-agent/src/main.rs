use clap::Parser;
use cml_chain::transaction::Transaction;
use cml_crypto::TransactionHash;
use either::Either;
use futures::channel::mpsc;
use futures::stream::{self, FuturesUnordered};
use futures::{Stream, StreamExt};
use log::{info, warn};
use std::future;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;
use tracing_subscriber::fmt::Subscriber;

use crate::account_events::AccountEventHandler;
use crate::account_index::AccountIndex;
use crate::config::AppConfig;
use crate::context::{ExecutionContext, MakerContext};
use crate::continuation_scanner::run_continuation_scanner;
use crate::deployment::{GreenDeployedValidators, GreenProtocolDeployment, GreenScriptHashes};
use crate::entity::EvolvingCardanoEntity;
use crate::funding_bootstrap::{bootstrap_funding_from_explorer, seed_funding_index};
use crate::http_intent_source::{router as http_intent_router, HttpIntentState};
use crate::intent_source::{run_tcp_intent_source, GreenIntentEvent};
use async_primitives::beacon::Beacon;
use bloom_offchain::execution_engine::backlog::SpecializedInterpreter;
use bloom_offchain::execution_engine::bundled::Bundled;
use bloom_offchain::execution_engine::execution_part_stream;
use bloom_offchain::execution_engine::funding_effect::FundingEvent;
use bloom_offchain::execution_engine::liquidity_book::TLB;
use bloom_offchain::execution_engine::multi_pair::MultiPair;
use bloom_offchain::execution_engine::storage::InMemoryStateIndex;
use bloom_offchain_cardano::event_sink::context::{HandlerContext, HandlerContextProto};
use bloom_offchain_cardano::event_sink::entity_index::InMemoryEntityIndex;
use bloom_offchain_cardano::event_sink::handler::{FundingEventHandler, LedgerCx, PairUpdateHandler};
use bloom_offchain_cardano::event_sink::order_index::InMemoryKvIndex;
use bloom_offchain_cardano::event_sink::tx_view::TxViewMut;
use bloom_offchain_cardano::execution_engine::interpreter::CardanoRecipeInterpreter;
use bloom_offchain_cardano::health::{
    health_tick_stream, AgentNodeStatus, EngineStatus, HealthMonitor, StreamId,
};
use bloom_offchain_cardano::http_endpoints::{create_health_router, HealthMonitorState};
use bloom_offchain_cardano::integrity::CheckIntegrity;
use bloom_offchain_cardano::orders::adhoc::AdhocFeeStructure;
use bloom_offchain_cardano::orders::green::GreenOrder;
use bloom_offchain_cardano::partitioning::select_partition;
use bloom_offchain_cardano::pools::royalty_v1::RoyaltyV1PoolOnly;
use bloom_offchain_cardano::validation_rules::ValidationRules;
use cardano_chain_sync::cache::LedgerCacheRocksDB;
use cardano_chain_sync::chain_sync_stream_with_health_monitor;
use cardano_chain_sync::client::ChainSyncClient;
use cardano_chain_sync::data::LedgerTxEvent;
use cardano_chain_sync::event_source::ledger_transactions;
use cardano_chain_sync::ChainSyncHealth;
use cardano_explorer::AnyExplorer;
use cardano_mempool_sync::client::LocalTxMonitorClient;
use cardano_mempool_sync::data::MempoolUpdate;
use cardano_mempool_sync::mempool_stream;
use cml_chain::builders::tx_builder::SignedTxBuilder;
use spectrum_cardano_lib::constants::{CONWAY_ERA_ID, SAFE_BLOCK_TIME};
use spectrum_cardano_lib::ex_units::ExUnits;
use spectrum_cardano_lib::output::FinalizedTxOut;
use spectrum_cardano_lib::{OutputRef, Token};
use spectrum_offchain::backlog::{BacklogCapacity, HotPriorityBacklog};
use spectrum_offchain::clock::SystemClock;
use spectrum_offchain::domain::event::{Channel, Transition};
use spectrum_offchain::domain::order::OrderUpdate;
use spectrum_offchain::domain::Baked;
use spectrum_offchain::event_sink::event_handler::{forward_with, EventHandler};
use spectrum_offchain::event_sink::process_events;
use spectrum_offchain::partitioning::Partitioned;
use spectrum_offchain::reporting::{reporting_stream, ReportingAgent};
use spectrum_offchain_cardano::collateral::pull_collateral;
use spectrum_offchain_cardano::creds::operator_creds;
use spectrum_offchain_cardano::data::dao_request::DAOContext;
use spectrum_offchain_cardano::data::order::Order;
use spectrum_offchain_cardano::data::pair::PairId;
use spectrum_offchain_cardano::deployment::ProtocolScriptHashes;
use spectrum_offchain_cardano::prover::operator::OperatorProver;
use spectrum_offchain_cardano::tx_submission::{tx_submission_agent_stream, TxSubmissionAgent};
use spectrum_offchain_cardano::tx_tracker::{new_tx_tracker_bundle, TxTrackerChannel};
use spectrum_streaming::{run_stream, StreamExt as StreamExtAlt};

mod account_events;
mod account_index;
mod account_store;
mod config;
mod context;
mod continuation_scanner;
mod deployment;
mod entity;
mod funding_bootstrap;
mod http_intent_source;
mod intent_source;
mod mpf;

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() {
    let subscriber = Subscriber::new();
    tracing::subscriber::set_global_default(subscriber).expect("setting tracing default failed");
    let args = AppArgs::parse();
    let raw_config = std::fs::read_to_string(args.config_path).expect("Cannot load configuration file");
    let config: AppConfig = serde_json::from_str(&raw_config).expect("Invalid configuration file");
    let config_integrity_violations = config.check_integrity();
    if !config_integrity_violations.is_empty() {
        panic!("Malformed configuration: {}", config_integrity_violations);
    }

    let raw_deployment = std::fs::read_to_string(args.deployment_path).expect("Cannot load deployment file");
    let deployment: GreenDeployedValidators =
        serde_json::from_str(&raw_deployment).expect("Invalid green deployment file");

    let raw_validation_rules =
        std::fs::read_to_string(args.validation_rules_path).expect("Cannot load bounds file");
    let validation_rules: ValidationRules =
        serde_json::from_str(&raw_validation_rules).expect("Invalid bounds file");

    log4rs::init_file(args.log4rs_path, Default::default()).unwrap();

    info!("Starting Green Order Off-Chain Agent ..");

    let state_synced = Beacon::relaxed(false);
    let rollback_in_progress = Beacon::strong(false);

    let explorer = AnyExplorer::new(&config.explorer, config.network_id)
        .await
        .expect("Explorer initialization failed");

    let green_deployment = GreenProtocolDeployment::unsafe_pull(deployment, &explorer).await;
    let protocol_deployment = green_deployment.spectrum.clone();

    let account_store_path = format!("{}.green-account-stores.json", config.chain_sync.db_path);
    let chain_sync_cache = Arc::new(Mutex::new(LedgerCacheRocksDB::new(config.chain_sync.db_path)));
    let chain_sync = ChainSyncClient::init(
        Arc::clone(&chain_sync_cache),
        config.node.path.clone(),
        config.node.magic,
        config.chain_sync.starting_point,
    )
    .await
    .expect("ChainSync initialization failed");

    // n2c clients:
    let mempool_sync =
        LocalTxMonitorClient::<Transaction>::connect(config.node.path.clone(), config.node.magic)
            .await
            .expect("MempoolSync initialization failed");

    let (failed_txs_snd, failed_txs_recv) = mpsc::channel(config.tx_submission_buffer_size);
    let (confirmed_txs_snd, confirmed_txs_recv) = mpsc::channel(config.tx_submission_buffer_size);
    let max_confirmation_delay_blocks = config.event_cache_ttl.as_secs() / SAFE_BLOCK_TIME.as_secs();
    let (tx_tracker_agent, tx_tracker_channel) = new_tx_tracker_bundle(
        confirmed_txs_recv,
        failed_txs_snd,
        config.tx_submission_buffer_size,
        max_confirmation_delay_blocks,
    );

    let (tx_submission_agent, tx_submission_channel) =
        TxSubmissionAgent::<CONWAY_ERA_ID, Transaction, TxTrackerChannel<TransactionHash, Transaction>>::new(
            tx_tracker_channel.clone(),
            config.node.clone(),
            config.tx_submission_buffer_size,
        )
        .await
        .expect("LocalTxSubmission initialization failed");
    let tx_submission_stream = tx_submission_agent_stream(tx_submission_agent);

    let (reporting_agent, reporting_channel) =
        ReportingAgent::new(config.reporting_endpoint, config.tx_submission_buffer_size).await;
    let reporting_stream = reporting_stream(reporting_agent);

    let (operator_paycred, collateral_address, funding_addresses) =
        operator_creds(config.operator_key.as_str(), config.network_id);

    info!(
        "Expecting collateral at {}",
        collateral_address.clone().address().to_bech32(None).unwrap()
    );

    let collateral = pull_collateral(collateral_address, &explorer)
        .await
        .expect("Couldn't retrieve collateral");

    let (pair_upd_snd_p1, pair_upd_recv_p1) = mpsc::channel::<(
        PairId,
        Channel<Transition<EvolvingCardanoEntity>, LedgerCx>,
    )>(config.event_feed_buffer_size);
    let (pair_upd_snd_p2, pair_upd_recv_p2) = mpsc::channel::<(
        PairId,
        Channel<Transition<EvolvingCardanoEntity>, LedgerCx>,
    )>(config.event_feed_buffer_size);
    let (pair_upd_snd_p3, pair_upd_recv_p3) = mpsc::channel::<(
        PairId,
        Channel<Transition<EvolvingCardanoEntity>, LedgerCx>,
    )>(config.event_feed_buffer_size);
    let (pair_upd_snd_p4, pair_upd_recv_p4) = mpsc::channel::<(
        PairId,
        Channel<Transition<EvolvingCardanoEntity>, LedgerCx>,
    )>(config.event_feed_buffer_size);

    let intent_pair_upd_snd =
        Partitioned::<4, PairId, futures::channel::mpsc::Sender<GreenIntentEvent>>::new([
            pair_upd_snd_p1.clone(),
            pair_upd_snd_p2.clone(),
            pair_upd_snd_p3.clone(),
            pair_upd_snd_p4.clone(),
        ]);
    let partitioned_pair_upd_snd =
        Partitioned::new([pair_upd_snd_p1, pair_upd_snd_p2, pair_upd_snd_p3, pair_upd_snd_p4]);

    let (funding_upd_snd_p1, funding_upd_recv_p1) =
        mpsc::channel::<FundingEvent<FinalizedTxOut>>(config.event_feed_buffer_size);
    let (funding_upd_snd_p2, funding_upd_recv_p2) =
        mpsc::channel::<FundingEvent<FinalizedTxOut>>(config.event_feed_buffer_size);
    let (funding_upd_snd_p3, funding_upd_recv_p3) =
        mpsc::channel::<FundingEvent<FinalizedTxOut>>(config.event_feed_buffer_size);
    let (funding_upd_snd_p4, funding_upd_recv_p4) =
        mpsc::channel::<FundingEvent<FinalizedTxOut>>(config.event_feed_buffer_size);

    let partitioned_funding_event_snd = Partitioned::new([
        funding_upd_snd_p1,
        funding_upd_snd_p2,
        funding_upd_snd_p3,
        funding_upd_snd_p4,
    ]);

    let entity_index = Arc::new(Mutex::new(InMemoryEntityIndex::new(config.event_cache_ttl)));
    let account_index = Arc::new(StdMutex::new(AccountIndex::with_persistence_path(
        account_store_path.into(),
    )));
    let account_event_handler = AccountEventHandler::new(
        Arc::clone(&account_index),
        GreenScriptHashes::from(&green_deployment),
    );
    if let Some(intent_listen_addr) = config.green_orders.intent_source.listen_addr {
        info!("Green intent source listening on {}", intent_listen_addr);
        tokio::spawn(run_tcp_intent_source(
            intent_listen_addr,
            Arc::clone(&account_index),
            GreenScriptHashes::from(&green_deployment),
            config.green_orders,
            intent_pair_upd_snd.clone(),
        ));
    }
    if let Some(http_listen_addr) = config.green_orders.intent_source.http_listen_addr {
        info!("Green HTTP intent source listening on {}", http_listen_addr);
        let app = http_intent_router(HttpIntentState {
            account_index: Arc::clone(&account_index),
            ctx: GreenScriptHashes::from(&green_deployment),
            config: config.green_orders,
            events: intent_pair_upd_snd.clone(),
        });
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(http_listen_addr)
                .await
                .expect("failed to bind green HTTP intent source");
            axum::serve(listener, app)
                .await
                .expect("green HTTP intent source failed");
        });
    }
    if config.green_orders.allow_partial {
        tokio::spawn(run_continuation_scanner(
            Arc::clone(&account_index),
            intent_pair_upd_snd,
            std::time::Duration::from_secs(1),
        ));
    }
    let funding_index = Arc::new(Mutex::new(
        InMemoryKvIndex::new(config.event_cache_ttl, SystemClock).with_tracing("funding_index"),
    ));
    let bootstrapped_funding = bootstrap_funding_from_explorer(
        &explorer,
        funding_addresses.clone(),
        collateral.reference(),
        config.min_operator_funding_lovelace,
        100,
    )
    .await;
    seed_funding_index(Arc::clone(&funding_index), &bootstrapped_funding).await;
    let mut bootstrap_funding_p1 = Vec::new();
    let mut bootstrap_funding_p2 = Vec::new();
    let mut bootstrap_funding_p3 = Vec::new();
    let mut bootstrap_funding_p4 = Vec::new();
    for (partition, event) in bootstrapped_funding {
        match partition {
            0 => bootstrap_funding_p1.push(event),
            1 => bootstrap_funding_p2.push(event),
            2 => bootstrap_funding_p3.push(event),
            3 => bootstrap_funding_p4.push(event),
            _ => unreachable!("funding bootstrap returned an out-of-range partition"),
        }
    }
    let dao_ctx: DAOContext = config.dao_config.clone().into();
    let handler_context = HandlerContextProto {
        executor_cred: operator_paycred,
        scripts: ProtocolScriptHashes::from(&protocol_deployment),
        adhoc_fee_structure: AdhocFeeStructure::empty(),
        validation_rules,
        dao_context: dao_ctx,
        graduated_pool_fee_config: Default::default(),
        snek_pool_script_hashes: Default::default(),
        graduated_pool_store: Default::default(),
        snek_pool_input_tracker: Default::default(),
        graduation_state: None,
    };
    let general_upd_handler: PairUpdateHandler<4, _, _, _, _, HandlerContextProto, HandlerContext<Token>> =
        PairUpdateHandler::new(
            partitioned_pair_upd_snd,
            Arc::clone(&entity_index),
            handler_context.clone(),
        );
    let funding_event_handler = FundingEventHandler::new(
        partitioned_funding_event_snd,
        funding_addresses.clone(),
        collateral.reference(), // collateral cannot be used for funding.
        funding_index,
    );

    info!("Derived funding addresses: {}", funding_addresses);

    let handlers_ledger: Vec<Box<dyn EventHandler<LedgerTxEvent<TxViewMut>> + Send>> = vec![
        Box::new(account_event_handler.clone()),
        Box::new(general_upd_handler.clone()),
        Box::new(funding_event_handler.clone()),
        Box::new(forward_with(confirmed_txs_snd, succinct_tx)),
    ];

    let handlers_mempool: Vec<Box<dyn EventHandler<MempoolUpdate<TxViewMut>> + Send>> = vec![
        Box::new(account_event_handler),
        Box::new(general_upd_handler),
        Box::new(funding_event_handler),
    ];

    let prover = OperatorProver::new(config.operator_key);
    let recipe_interpreter = CardanoRecipeInterpreter::new(config.take_residual_fee);
    let spec_interpreter = NoopSpecializedInterpreter;
    let maker_context = MakerContext {
        time: 0.into(),
        execution_conf: config
            .execution
            .into_lb_config(validation_rules.limit_order.min_cost_per_ex_step.into()),
        backlog_capacity: BacklogCapacity::from(config.backlog_capacity),
    };
    let context_p1 = ExecutionContext {
        time: 0.into(),
        deployment: green_deployment.clone(),
        reward_addr: funding_addresses[0].clone().into(),
        backlog_capacity: BacklogCapacity::from(config.backlog_capacity),
        collateral: collateral.clone(),
        network_id: config.network_id,
        operator_cred: operator_paycred,
        dao_ctx,
        royalty_context: config.royalty_withdraw,
        account_index: Arc::clone(&account_index),
        allow_partial: config.green_orders.allow_partial,
    };
    let context_p2 = ExecutionContext {
        time: 0.into(),
        deployment: green_deployment.clone(),
        reward_addr: funding_addresses[1].clone().into(),
        backlog_capacity: BacklogCapacity::from(config.backlog_capacity),
        collateral: collateral.clone(),
        network_id: config.network_id,
        operator_cred: operator_paycred,
        dao_ctx,
        royalty_context: config.royalty_withdraw,
        account_index: Arc::clone(&account_index),
        allow_partial: config.green_orders.allow_partial,
    };
    let context_p3 = ExecutionContext {
        time: 0.into(),
        deployment: green_deployment.clone(),
        reward_addr: funding_addresses[2].clone().into(),
        backlog_capacity: BacklogCapacity::from(config.backlog_capacity),
        collateral: collateral.clone(),
        network_id: config.network_id,
        operator_cred: operator_paycred,
        dao_ctx,
        royalty_context: config.royalty_withdraw,
        account_index: Arc::clone(&account_index),
        allow_partial: config.green_orders.allow_partial,
    };
    let context_p4 = ExecutionContext {
        time: 0.into(),
        deployment: green_deployment,
        reward_addr: funding_addresses[3].clone().into(),
        backlog_capacity: BacklogCapacity::from(config.backlog_capacity),
        collateral,
        network_id: config.network_id,
        operator_cred: operator_paycred,
        dao_ctx,
        royalty_context: config.royalty_withdraw,
        account_index: Arc::clone(&account_index),
        allow_partial: config.green_orders.allow_partial,
    };

    let multi_book =
        MultiPair::new::<TLB<GreenOrder, RoyaltyV1PoolOnly, PairId, ExUnits>>(maker_context.clone(), "Book");
    let multi_backlog =
        MultiPair::new::<HotPriorityBacklog<Bundled<Order, FinalizedTxOut>>>(maker_context, "Backlog");
    let state_index = InMemoryStateIndex::with_tracing();

    const NUM_ENGINE_STREAMS: usize = 4;
    let (engine_tx, engine_rx) = mpsc::unbounded::<(StreamId, EngineStatus)>();

    let execution_stream_p1 = execution_part_stream(
        state_index.clone(),
        multi_book.clone(),
        multi_backlog.clone(),
        context_p1,
        recipe_interpreter,
        spec_interpreter,
        prover.clone(),
        adapt_events(
            select_partition(pair_upd_recv_p1, config.partitioning.clone())
                .buffered_within(config.event_feed_buffering_duration),
        ),
        stream::iter(bootstrap_funding_p1).chain(funding_upd_recv_p1),
        tx_submission_channel.clone(),
        reporting_channel.clone(),
        state_synced.clone(),
        rollback_in_progress.clone(),
        0u8,
        engine_tx.clone(),
    );
    let execution_stream_p2 = execution_part_stream(
        state_index.clone(),
        multi_book.clone(),
        multi_backlog.clone(),
        context_p2,
        recipe_interpreter,
        spec_interpreter,
        prover.clone(),
        adapt_events(
            select_partition(pair_upd_recv_p2, config.partitioning.clone())
                .buffered_within(config.event_feed_buffering_duration),
        ),
        stream::iter(bootstrap_funding_p2).chain(funding_upd_recv_p2),
        tx_submission_channel.clone(),
        reporting_channel.clone(),
        state_synced.clone(),
        rollback_in_progress.clone(),
        1u8,
        engine_tx.clone(),
    );
    let execution_stream_p3 = execution_part_stream(
        state_index.clone(),
        multi_book.clone(),
        multi_backlog.clone(),
        context_p3,
        recipe_interpreter,
        spec_interpreter,
        prover.clone(),
        adapt_events(
            select_partition(pair_upd_recv_p3, config.partitioning.clone())
                .buffered_within(config.event_feed_buffering_duration),
        ),
        stream::iter(bootstrap_funding_p3).chain(funding_upd_recv_p3),
        tx_submission_channel.clone(),
        reporting_channel.clone(),
        state_synced.clone(),
        rollback_in_progress.clone(),
        2u8,
        engine_tx.clone(),
    );
    let execution_stream_p4 = execution_part_stream(
        state_index,
        multi_book,
        multi_backlog,
        context_p4,
        recipe_interpreter,
        spec_interpreter,
        prover,
        adapt_events(
            select_partition(pair_upd_recv_p4, config.partitioning)
                .buffered_within(config.event_feed_buffering_duration),
        ),
        stream::iter(bootstrap_funding_p4).chain(funding_upd_recv_p4),
        tx_submission_channel,
        reporting_channel,
        state_synced.clone(),
        rollback_in_progress.clone(),
        3u8,
        engine_tx.clone(),
    );

    let (node_to_health_snd, node_to_health_recv) = mpsc::unbounded::<ChainSyncHealth>();

    let ledger_stream = Box::pin(ledger_transactions(
        chain_sync_cache,
        chain_sync_stream_with_health_monitor(chain_sync, state_synced.clone(), node_to_health_snd),
        config.chain_sync.disable_rollbacks_until,
        config.chain_sync.replay_from_point,
        rollback_in_progress,
    ))
    .await
    .map(|ev| ev.map(TxViewMut::from));

    let node_status_stream = node_to_health_recv.map(|_| AgentNodeStatus::ok());

    let mempool_stream = mempool_stream(mempool_sync, tx_tracker_channel, failed_txs_recv, state_synced)
        .map(|ev| ev.map(TxViewMut::from));

    let process_ledger_events_stream = process_events(ledger_stream, handlers_ledger);
    let process_mempool_events_stream = process_events(mempool_stream, handlers_mempool);

    let processes = FuturesUnordered::new();

    let process_ledger_events_stream_handle = tokio::spawn(run_stream(process_ledger_events_stream));
    processes.push(process_ledger_events_stream_handle);

    let process_mempool_events_stream_handle = if !config.disable_mempool {
        tokio::spawn(run_stream(process_mempool_events_stream))
    } else {
        tokio::spawn(future::ready(()))
    };
    processes.push(process_mempool_events_stream_handle);

    let execution_stream_p1_handle = tokio::spawn(run_stream(execution_stream_p1));
    processes.push(execution_stream_p1_handle);

    let execution_stream_p2_handle = tokio::spawn(run_stream(execution_stream_p2));
    processes.push(execution_stream_p2_handle);

    let execution_stream_p3_handle = tokio::spawn(run_stream(execution_stream_p3));
    processes.push(execution_stream_p3_handle);

    let execution_stream_p4_handle = tokio::spawn(run_stream(execution_stream_p4));
    processes.push(execution_stream_p4_handle);

    let tx_submission_stream_handle = tokio::spawn(run_stream(tx_submission_stream));
    processes.push(tx_submission_stream_handle);

    let reporting_stream_handle = tokio::spawn(run_stream(reporting_stream));
    processes.push(reporting_stream_handle);

    let tx_tracker_handle = tokio::spawn(tx_tracker_agent.run());
    processes.push(tx_tracker_handle);

    let (health_api_snd, health_api_recv) =
        mpsc::unbounded::<bloom_offchain_cardano::health::GetHealth<EngineStatus, AgentNodeStatus>>();
    let (health_tick_rx, health_tick_driver) = health_tick_stream();
    processes.push(tokio::spawn(health_tick_driver));
    let health_monitor = HealthMonitor::<_, _, _, _, EngineStatus, AgentNodeStatus>::new(
        engine_rx,
        node_status_stream,
        health_api_recv,
        health_tick_rx,
        NUM_ENGINE_STREAMS,
    );
    processes.push(tokio::spawn(health_monitor));
    if let Some(addr) = config.health_listen_addr {
        let health_state = HealthMonitorState {
            sender: health_api_snd,
        };
        let router = create_health_router(health_state);
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .expect("Failed to bind health server");
        info!("Health API listening on http://{}", addr);
        processes.push(tokio::spawn(async move {
            axum::serve(listener, router).await.expect("Health server failed")
        }));
    } else {
        warn!("Health listen address not configured; health API disabled");
    }

    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_panic(info);
        std::process::exit(1);
    }));

    run_stream(processes).await;
}

fn succinct_tx(tx: LedgerTxEvent<TxViewMut>) -> (TransactionHash, u64) {
    let (LedgerTxEvent::TxApplied { tx, block_number, .. }
    | LedgerTxEvent::TxUnapplied { tx, block_number, .. }) = tx;
    (tx.hash, block_number)
}

fn adapt_events(
    xs: impl Stream<Item = (PairId, Channel<Transition<EvolvingCardanoEntity>, LedgerCx>)> + Unpin,
) -> impl Stream<
    Item = (
        PairId,
        Either<
            Channel<
                Transition<
                    Bundled<
                        Either<Baked<GreenOrder, OutputRef>, Baked<RoyaltyV1PoolOnly, OutputRef>>,
                        FinalizedTxOut,
                    >,
                >,
                LedgerCx,
            >,
            Channel<OrderUpdate<Bundled<Order, FinalizedTxOut>, Order>, LedgerCx>,
        >,
    ),
> {
    xs.map(|(p, m)| (p, Either::Left(m.map(|s| s.map(|EvolvingCardanoEntity(e)| e)))))
}

#[derive(Parser)]
#[command(name = "green-order-cardano-agent")]
#[command(author = "Spectrum Labs")]
#[command(version = "1.0.0")]
#[command(about = "Green Order Off-Chain Agent", long_about = None)]
struct AppArgs {
    /// Path to the JSON configuration file.
    #[arg(long, short)]
    config_path: String,
    /// Path to the deployment JSON configuration file .
    #[arg(long, short)]
    deployment_path: String,
    /// Path to the bounds JSON configuration file .
    #[arg(long, short)]
    validation_rules_path: String,
    /// Path to the log4rs YAML configuration file.
    #[arg(long, short)]
    log4rs_path: String,
}

#[derive(Debug, Copy, Clone)]
struct NoopSpecializedInterpreter;

impl
    SpecializedInterpreter<
        RoyaltyV1PoolOnly,
        Order,
        OutputRef,
        SignedTxBuilder,
        FinalizedTxOut,
        ExecutionContext,
    > for NoopSpecializedInterpreter
{
    fn try_run(
        &mut self,
        _pool: Bundled<RoyaltyV1PoolOnly, FinalizedTxOut>,
        _order: Bundled<Order, FinalizedTxOut>,
        _context: ExecutionContext,
    ) -> Option<(
        SignedTxBuilder,
        Bundled<Baked<RoyaltyV1PoolOnly, OutputRef>, FinalizedTxOut>,
        Bundled<Order, FinalizedTxOut>,
    )> {
        None
    }
}
