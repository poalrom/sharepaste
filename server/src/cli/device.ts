import { DeviceCredentials } from "../server/device-credentials.js";
import { withRepo } from "./db.js";

export const runDeviceList = (args: { dbPath: string }) =>
  withRepo(args.dbPath, (repo) => DeviceCredentials.listForOperator(repo));

export const runDeviceRevoke = (args: { dbPath: string; deviceId: string }): void => {
  withRepo(args.dbPath, (repo) => {
    const found = DeviceCredentials.revokeEverywhere(repo, args.deviceId, Date.now());
    if (found === 0) throw new Error(`device ${args.deviceId} not found`);
  });
};
