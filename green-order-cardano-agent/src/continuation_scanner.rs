use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::channel::mpsc;
use futures::SinkExt;
use spectrum_offchain::partitioning::Partitioned;
use spectrum_offchain_cardano::data::pair::PairId;

use crate::account_index::AccountIndex;
use crate::intent_source::{admitted_intent_to_event, GreenIntentEvent};

pub async fn run_continuation_scanner(
    account_index: Arc<Mutex<AccountIndex>>,
    mut events: Partitioned<4, PairId, mpsc::Sender<GreenIntentEvent>>,
    interval: Duration,
) {
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        let continuations = {
            let index = account_index.lock().expect("account index lock poisoned");
            index.pending_continuations()
        };
        if !continuations.is_empty() {
            log::info!(
                "Green continuation scanner found {} pending continuation(s)",
                continuations.len()
            );
        }
        for continuation in continuations {
            let event = admitted_intent_to_event(continuation);
            let pair = event.0;
            log::info!("Emitting green continuation intent for pair {:?}", pair);
            if let Err(err) = events.get_mut(pair).send(event).await {
                log::warn!(
                    "Failed to emit green continuation intent for pair {:?}: {}",
                    pair,
                    err
                );
            }
        }
    }
}
