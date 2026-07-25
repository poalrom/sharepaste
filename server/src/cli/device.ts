import { withRepo } from "./db.js";

export const runDeviceList = (args: { dbPath: string }) =>
  withRepo(args.dbPath, (repo) => repo.memberships.listAll());

export const runDeviceRevoke = (args: { dbPath: string; deviceId: string }): void => {
  withRepo(args.dbPath, (repo) => {
    const all = repo.memberships.listAll().filter((m) => m.device_id === args.deviceId);
    if (all.length === 0) throw new Error(`device ${args.deviceId} not found`);
    for (const m of all) {
      repo.memberships.revoke(m.user_id, m.device_id, Date.now());
    }
  });
};
