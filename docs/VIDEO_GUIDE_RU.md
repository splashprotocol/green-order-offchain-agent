# Руководство по записи close-out видео для Milestone 5

Это руководство объясняет, как записать видео для Catalyst-аудиторов по Milestone 5. Оно написано как пошаговый сценарий: что открыть, что показать, что сказать и что проверить перед отправкой.

Основной английский скрипт находится здесь:

https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/DEMO.md

## Цель видео

Видео должно показать аудиторам, что проект реально завершен и проверяем:

1. Репозиторий публичный и доступен.
2. Код Green Order offchain agent находится в репозитории.
3. TypeScript SDK находится в репозитории и используется для общения с агентом.
4. Есть тесты и документация.
5. Есть live/preprod demo flow или отчет уже выполненного demo run.
6. Close-out report содержит ссылки на код, тесты, документацию и инструкции проверки.

Видео не должно раскрывать приватные данные: seed-фразы, приватные ключи, Blockfrost API key, HMAC secret, локальные пути к socket-файлам или содержимое `.state`.

## Перед записью

Откройте браузер и терминал заранее.

В браузере подготовьте вкладки:

- Репозиторий: https://github.com/splashprotocol/green-order-offchain-agent
- Branch `sdk-api`: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api
- Close-out report: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/CLOSEOUT_REPORT.md
- Architecture: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/ARCHITECTURE.md
- Testing: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/TESTING.md
- Demo script: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/DEMO.md
- SDK package: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/sdk/typescript
- Agent source: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/green-order-cardano-agent/src
- Demo wrapper: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/demo

В терминале подготовьте репозиторий:

```sh
cd green-order-offchain-agent
git checkout sdk-api
git status --short --branch
```

Если видео будет показывать live preprod run, заранее проверьте, что есть:

- preprod node socket;
- funded wallet seed или возможность отправить tADA на адрес, который попросит demo script;
- Blockfrost key или рабочий Koios fallback, если это требуется вашей локальной среде;
- достаточно времени, потому что live preprod indexing и подтверждение транзакций могут занимать минуты.

Если live run занимает слишком долго, нормально показать уже созданный `demo-report.md`, но нужно объяснить, что это результат выполнения `./demo/catalyst-demo.sh`.

## Рекомендуемая структура видео

Оптимальная длина: 8-15 минут. Не нужно подробно читать весь код. Нужно показать, что все артефакты существуют и что аудитор может сам их проверить.

### 1. Вступление

Откройте репозиторий:

https://github.com/splashprotocol/green-order-offchain-agent

Скажите:

```text
This is the public repository for the Green Order offchain agent project.
For Milestone 5 close-out we provide the final report, architecture documentation,
testing instructions, demo script, TypeScript SDK, and the preprod demo wrapper.
The milestone branch is sdk-api.
```

Покажите, что открыт именно публичный репозиторий `splashprotocol/green-order-offchain-agent`, а не личный fork.

### 2. Показать close-out report

Откройте:

https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/CLOSEOUT_REPORT.md

Скажите:

```text
This close-out report follows the Catalyst proof-of-achievement format.
Each milestone output has a description, evidence links, and concrete verification steps.
The report links only to public GitHub resources.
```

Покажите разделы:

- `Milestone Output 1 — Completed development and testing`
- `Milestone Output 2 — Comprehensive documentation published as Markdown`
- `Milestone Output 3 — Project completion report`
- `Milestone Output 4 — Video demonstration`
- `SDK and off-chain communication evidence`

Если финальный YouTube/Vimeo URL еще не вставлен, не скрывайте это. Скажите:

```text
The final video URL will be inserted into the pending video line before Catalyst submission.
```

### 3. Показать архитектуру

Откройте:

https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/ARCHITECTURE.md

Скажите:

```text
The agent observes Cardano preprod state, tracks Aleph account UTxOs,
accepts signed Green Order intents, plans execution against Royalty V1 pools,
and exposes a loopback HTTP API for SDK-based integration.
```

Покажите по документу, что описаны:

- agent runtime;
- account indexing;
- account store and MPF state;
- SDK/API communication;
- demo wrapper.

Не нужно объяснять все детали протокола. Цель видео - доказать, что документация существует и аудитор может ее проверить.

### 4. Показать код агента

Откройте эти файлы:

- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/main.rs
- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/http_intent_source.rs
- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/account_index.rs
- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/account_store.rs

Что сказать:

```text
These files contain the delivered Rust offchain agent.
main.rs wires the runtime.
http_intent_source.rs exposes the local HTTP endpoints for intents, account binding,
monitoring and SDK access.
account_index.rs and account_store.rs maintain the observed Aleph account state
and verify planned state transitions.
```

Покажите в `http_intent_source.rs`, что есть endpoints и HMAC-related logic, если удобно. Не надо читать все функции.

### 5. Показать TypeScript SDK

Откройте:

- https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/sdk/typescript
- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/client.ts
- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/intent.ts
- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/account.ts
- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/hmac.ts

Что сказать:

```text
The TypeScript SDK is the developer-facing integration layer.
It wraps agent monitoring, account status, account binding, intent submission,
and optional HMAC signing for authenticated local agent requests.
```

Покажите `src/hmac.ts` и скажите:

```text
For the e2e run, the HMAC secret is generated per run by the harness.
It is not hardcoded in the repository and is not shown in the report.
```

### 6. Показать SDK-based E2E scripts

Откройте:

- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/11-query-agent-via-sdk.ts
- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/12-query-agent-with-bad-hmac.ts

Что сказать:

```text
The preprod flow does not call the agent only by raw curl.
It includes SDK-based scripts that query monitoring and account endpoints.
It also includes a negative HMAC check proving that an invalid signature is rejected with Forbidden.
```

Покажите, что `12-query-agent-with-bad-hmac.ts` ожидает rejection. Это важно для аудиторов: HMAC не просто написан в SDK, он проверяется агентом.

### 7. Показать тесты

Откройте:

https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/TESTING.md

Скажите:

```text
Testing instructions are documented here.
The repository includes Rust unit tests for account indexing, account store,
and HTTP intent source, TypeScript SDK tests, preprod script tests,
and demo wrapper tests.
```

Если есть время, выполните локальные команды на видео:

```sh
cargo test -q -p green-order-cardano-agent account_index
cargo test -q -p green-order-cardano-agent account_store
cargo test -q -p green-order-cardano-agent http_intent_source
npm --prefix sdk/typescript test
bash demo/tests/run-tests.sh
```

Если времени мало, выполните хотя бы:

```sh
npm --prefix sdk/typescript test
bash demo/tests/run-tests.sh
```

После выполнения покажите, что команды завершились успешно.

### 8. Показать live/preprod demo flow

Откройте:

- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/demo/README.md
- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/demo/AUDITOR.md
- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/demo/SCRIPT_REFERENCE.md

Скажите:

```text
For auditors, the repository provides a single demo wrapper.
It prepares a fresh run-local state, creates or waits for funded preprod wallets,
starts the agent, submits full-fill and partial-fill intents,
queries the agent through the SDK, verifies HMAC rejection,
and writes a demo report with transaction hashes and checkpoints.
```

Команда:

```sh
./demo/catalyst-demo.sh
```

Если запускаете live на видео, объясняйте каждый checkpoint:

- fresh wallets are generated;
- script prints a funding address if funding is needed;
- after funding is observed, setup continues;
- agent starts with generated run-local config;
- account is created and bound;
- pool and separator setup is prepared;
- full-fill intent is submitted;
- partial-fill intent is submitted under partial-enabled config;
- SDK monitoring queries are executed;
- bad HMAC request is rejected;
- `demo-report.md` is generated.

Если не запускаете live полностью, покажите уже готовый report и скажите:

```text
This is the generated demo report from a completed preprod run.
It contains the transaction hashes and verification checkpoints produced by the demo wrapper.
```

Не показывайте seed-фразы, приватные ключи, API keys и содержимое `.state`.

### 9. Финальное закрытие видео

Вернитесь к:

https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/CLOSEOUT_REPORT.md

Скажите:

```text
This completes the Milestone 5 close-out evidence.
The public repository contains the delivered code, SDK, tests, documentation,
demo instructions, and this completion report.
The final public video URL will be inserted into the report before submission.
```

## Что обязательно должно быть видно в видео

- URL публичного репозитория `https://github.com/splashprotocol/green-order-offchain-agent`.
- Branch `sdk-api`.
- `docs/CLOSEOUT_REPORT.md`.
- `docs/ARCHITECTURE.md`.
- `docs/TESTING.md`.
- Rust agent files.
- `sdk/typescript`.
- SDK HMAC helper.
- SDK-based preprod scripts.
- Demo wrapper under `demo`.
- Тестовые команды или результат их выполнения.
- Live/preprod demo command или generated demo report.

## Чего нельзя показывать

Не показывайте:

- seed-фразы;
- private keys;
- Blockfrost API key;
- HMAC secret;
- локальный node socket path;
- содержимое `.state`;
- приватные GitHub remotes или tokens;
- терминальные команды, где secrets передаются прямо в строке.

Если секрет случайно попал на экран, видео лучше перезаписать.

## Короткая версия сценария на английском

Если нужно читать с экрана, используйте этот текст:

```text
This is the public Green Order offchain agent repository for Catalyst Milestone 5.
The milestone branch is sdk-api.

The close-out report maps each milestone output to public evidence links and verification steps.
The architecture document explains the Rust agent, account indexing, account store,
SDK communication, and demo wrapper.

The Rust agent accepts signed Green Order intents, binds observed Aleph accounts,
tracks account state, and executes against Royalty V1 pools on Cardano preprod.

The TypeScript SDK provides developer-facing helpers for monitoring, account status,
account binding, intent submission, and HMAC-signed local agent requests.
The e2e scripts use this SDK and include a negative test proving that invalid HMAC
requests are rejected.

The testing document lists Rust, SDK, preprod script, and demo wrapper checks.
The demo wrapper runs the reviewer-facing preprod flow and generates a report
with transaction hashes and checkpoints.

This completes the project close-out evidence: public code, SDK, tests,
documentation, demo instructions, and completion report.
```

## Финальная проверка перед отправкой аудиторам

Перед отправкой Milestone 5 убедитесь:

1. Видео загружено на публичный YouTube или Vimeo URL.
2. URL видео вставлен в `docs/CLOSEOUT_REPORT.md` вместо pending line.
3. Все ссылки в отчете открываются из публичного репозитория.
4. В отчете и видео нет secrets.
5. Видео показывает публичный repo `splashprotocol/green-order-offchain-agent`, а не личный fork.
6. Branch в ссылках и в браузере - `sdk-api`.
