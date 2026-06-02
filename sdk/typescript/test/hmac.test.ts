import test from "node:test";
import assert from "node:assert/strict";

import { createHmacHeaders, hmacCanonicalString } from "../src/index.js";

test("builds stable HMAC canonical request strings", async () => {
  assert.equal(
    await hmacCanonicalString({
      method: "post",
      pathAndQuery: "/intents?x=1",
      timestamp: "1700000000000",
      nonce: "nonce-1",
      body: "{\"ok\":true}",
    }),
    [
      "POST",
      "/intents?x=1",
      "1700000000000",
      "nonce-1",
      "4062edaf750fb8074e7e83e0c9028c94e32468a8b6f1614774328ef045150f93",
    ].join("\n"),
  );
});

test("creates HMAC headers without exposing the secret", async () => {
  const headers = await createHmacHeaders({
    method: "POST",
    pathAndQuery: "/intents",
    body: "{}",
    secret: "top-secret",
    keyId: "demo",
    timestamp: "1700000000000",
    nonce: "nonce-1",
  });

  assert.equal(headers["x-go-key-id"], "demo");
  assert.equal(headers["x-go-timestamp"], "1700000000000");
  assert.equal(headers["x-go-nonce"], "nonce-1");
  assert.equal(headers["x-go-body-sha256"], "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a");
  assert.match(headers["x-go-signature"], /^hmac-sha256=[0-9a-f]{64}$/);
  assert.doesNotMatch(JSON.stringify(headers), /top-secret/);
});
