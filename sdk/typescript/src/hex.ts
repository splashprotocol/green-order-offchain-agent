export function assertHex(name: string, value: string, length?: number): void {
  if (!/^[0-9a-fA-F]*$/.test(value)) {
    throw new Error(`${name} must be hex`);
  }
  if (length !== undefined && value.length !== length) {
    throw new Error(`${name} must be ${length} hex chars`);
  }
}

export function hexToBytes(hex: string): Uint8Array {
  assertHex("hex", hex);
  if (hex.length % 2 !== 0) throw new Error("hex must have an even length");
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

export function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function utf8ToBytes(text: string): Uint8Array {
  return new TextEncoder().encode(text);
}
