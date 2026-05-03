import { createHash, randomBytes, randomUUID, timingSafeEqual } from "node:crypto";
import argon2 from "argon2";

export const randomId = (): string => randomUUID();

export const randomToken = (): string =>
  randomBytes(32).toString("base64url");

export const sha256Hex = (input: string | Buffer): string =>
  createHash("sha256").update(input).digest("hex");

const ARGON_OPTS: argon2.Options = {
  type: argon2.argon2id,
  memoryCost: 19_456, // 19 MiB
  timeCost: 2,
  parallelism: 1,
};

export const hashToken = (token: string): Promise<string> =>
  argon2.hash(token, ARGON_OPTS);

export const verifyToken = (hash: string, token: string): Promise<boolean> =>
  argon2.verify(hash, token);

export const timingSafeEqualHex = (a: string, b: string): boolean => {
  const al = a.toLowerCase();
  const bl = b.toLowerCase();
  if (al.length !== bl.length) return false;
  const ab = Buffer.from(al, "hex");
  const bb = Buffer.from(bl, "hex");
  if (ab.length !== bb.length || ab.length === 0) return false;
  return timingSafeEqual(ab, bb);
};
