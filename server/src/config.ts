export const DEFAULT_DB_PATH = "/var/lib/sharepaste/sharepaste.sqlite";

export interface ServeConfig {
  dbPath: string;
  port: number;
  host: string;
}

export const loadServeConfig = (env: NodeJS.ProcessEnv = process.env): ServeConfig => ({
  dbPath: env.DB_PATH ?? DEFAULT_DB_PATH,
  port: Number(env.PORT ?? 8443),
  host: env.HOST ?? "0.0.0.0",
});
