import { openDb } from "../db/index.js";
import { migrate } from "../db/migrate.js";
import { Repository } from "../db/repository.js";
import { randomId, randomToken, sha256Hex } from "../crypto.js";

export interface CreateUserArgs {
  dbPath: string;
  username: string;
  ttlSeconds: number;
}

export interface CreateUserResult {
  user_id: string;
  invite_token: string;
}

const open = (dbPath: string) => {
  const db = openDb(dbPath);
  migrate(db);
  return new Repository(db);
};

export const runUserCreate = (args: CreateUserArgs): CreateUserResult => {
  const repo = open(args.dbPath);
  const id = randomId();
  repo.users.create({ id, username: args.username });
  const token = randomToken();
  repo.invites.create({
    token_hash: sha256Hex(token),
    user_id: id,
    expires_at: Date.now() + args.ttlSeconds * 1000,
    claimed_at: null,
  });
  return { user_id: id, invite_token: token };
};

export const runUserList = (args: { dbPath: string }) => open(args.dbPath).users.list();

export const runUserDelete = (args: { dbPath: string; userId: string }) => {
  const repo = open(args.dbPath);
  repo.users.delete(args.userId);
};
