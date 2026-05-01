#!/usr/bin/env node
import { Command } from "commander";
import { runUserCreate, runUserList, runUserDelete } from "./cli/user.js";
import { runDeviceList, runDeviceRevoke } from "./cli/device.js";
import { runEntryPurge } from "./cli/entry.js";
import { startServer } from "./cli/serve.js";
import { loadServeConfig } from "./config.js";

const program = new Command()
  .name("sharepaste")
  .description("Sharepaste server and operator CLI")
  .version("0.1.0");

const dbPathOption = (cmd: Command) =>
  cmd.option(
    "--db <path>",
    "path to SQLite database file",
    process.env.DB_PATH ?? "/var/lib/sharepaste/sharepaste.sqlite"
  );

const user = program.command("user").description("User management");

dbPathOption(user.command("create"))
  .argument("<username>")
  .option("--ttl <seconds>", "invite TTL", "604800")
  .action(async (username: string, opts: { db: string; ttl: string }) => {
    const result = runUserCreate({
      dbPath: opts.db,
      username,
      ttlSeconds: Number(opts.ttl),
    });
    console.log(JSON.stringify(result, null, 2));
  });

dbPathOption(user.command("list")).action((opts: { db: string }) => {
  const rows = runUserList({ dbPath: opts.db });
  console.log(JSON.stringify(rows, null, 2));
});

dbPathOption(user.command("delete"))
  .argument("<user_id>")
  .action((userId: string, opts: { db: string }) => {
    runUserDelete({ dbPath: opts.db, userId });
  });

const device = program.command("device").description("Device management");
dbPathOption(device.command("list")).action((opts: { db: string }) => {
  console.log(JSON.stringify(runDeviceList({ dbPath: opts.db }), null, 2));
});
dbPathOption(device.command("revoke"))
  .argument("<device_id>")
  .action((deviceId: string, opts: { db: string }) => {
    runDeviceRevoke({ dbPath: opts.db, deviceId });
  });

const entry = program.command("entry").description("Entry administration");
dbPathOption(entry.command("purge"))
  .requiredOption("--user <user_id>", "user id")
  .action((opts: { db: string; user: string }) => {
    const removed = runEntryPurge({ dbPath: opts.db, userId: opts.user });
    console.log(JSON.stringify({ removed }, null, 2));
  });

program
  .command("serve")
  .description("Run HTTP server")
  .action(async () => {
    const cfg = loadServeConfig();
    const handle = await startServer({
      dbPath: cfg.dbPath,
      port: cfg.port,
      host: cfg.host,
    });
    console.log(`sharepaste serving on ${handle.url}`);
    const onSignal = async () => {
      await handle.close();
      process.exit(0);
    };
    process.on("SIGINT", onSignal);
    process.on("SIGTERM", onSignal);
  });

program.parseAsync(process.argv);
