import { randomId, randomToken, sha256Hex } from "../crypto.js";
import { withRepo } from "./db.js";

export interface CreateUserArgs {
  dbPath: string;
  username: string;
  ttlSeconds: number;
}

export interface CreateUserResult {
  user_id: string;
  invite_token: string;
}

export const runUserCreate = (args: CreateUserArgs): CreateUserResult =>
  withRepo(args.dbPath, (repo) => {
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
  });

export const runUserList = (args: { dbPath: string }) =>
  withRepo(args.dbPath, (repo) => repo.users.list());

export const runUserDelete = (args: { dbPath: string; userId: string }): void => {
  withRepo(args.dbPath, (repo) => {
    repo.users.delete(args.userId);
  });
};
