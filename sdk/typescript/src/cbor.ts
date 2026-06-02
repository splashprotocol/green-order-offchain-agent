import { bytesToHex, hexToBytes } from "./hex.js";

export function aikenConstr0Cbor(fields: Uint8Array[]): Uint8Array {
  return concatBytes(new Uint8Array([0xd8, 0x79, 0x9f]), ...fields, new Uint8Array([0xff]));
}

export function aikenTupleCbor(fields: Uint8Array[]): Uint8Array {
  return concatBytes(new Uint8Array([0x9f]), ...fields, new Uint8Array([0xff]));
}

export function aikenUintCbor(value: bigint): Uint8Array {
  if (value < 0n) throw new Error("Aiken intention fields must be non-negative");
  if (value <= 23n) return new Uint8Array([Number(value)]);
  if (value <= 0xffn) return new Uint8Array([0x18, Number(value)]);
  if (value <= 0xffffn) return uintWithHeader(0x19, value, 2);
  if (value <= 0xffffffffn) return uintWithHeader(0x1a, value, 4);
  if (value <= 0xffffffffffffffffn) return uintWithHeader(0x1b, value, 8);
  throw new Error("Aiken intention integer exceeds uint64");
}

export function aikenBytesCbor(bytes: Uint8Array): Uint8Array {
  const len = BigInt(bytes.length);
  let header: Uint8Array;
  if (len <= 23n) header = new Uint8Array([0x40 | Number(len)]);
  else if (len <= 0xffn) header = new Uint8Array([0x58, Number(len)]);
  else if (len <= 0xffffn) header = uintWithHeader(0x59, len, 2);
  else if (len <= 0xffffffffn) header = uintWithHeader(0x5a, len, 4);
  else header = uintWithHeader(0x5b, len, 8);
  return concatBytes(header, bytes);
}

export function hexBytesCbor(hex: string): Uint8Array {
  return aikenBytesCbor(hexToBytes(hex));
}

export function cborHex(bytes: Uint8Array): string {
  return bytesToHex(bytes);
}

function uintWithHeader(header: number, value: bigint, byteLength: number): Uint8Array {
  const out = new Uint8Array(1 + byteLength);
  out[0] = header;
  for (let i = byteLength; i > 0; i--) {
    out[i] = Number(value & 0xffn);
    value >>= 8n;
  }
  return out;
}

export function concatBytes(...chunks: Uint8Array[]): Uint8Array {
  const total = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    out.set(chunk, offset);
    offset += chunk.length;
  }
  return out;
}
