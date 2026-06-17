# SDK Test Coverage

The SDK test suite is located in `sdk/typescript/test`.

Run:

```sh
cd sdk/typescript
npm test
npm run test:coverage
```

Covered behavior:

- ADA and native asset wire encoding
- Aleph intention digest parity with the existing preprod/Rust fixture
- agent `POST /intents` payload construction
- signer abstraction behavior
- HMAC canonical request strings
- HMAC header generation without exposing secrets
- submit client request construction
- rejected agent response mapping to `GreenOrderSdkError`
- account status route wrapping
- account summary route wrapping
- monitoring summary and readiness route wrapping

Latest local verification:

```text
npm run test:coverage
tests 17
pass 17
fail 0
all files line coverage 92.33%
all files branch coverage 78.57%
all files function coverage 93.48%
```
