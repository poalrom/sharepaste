export interface ServeConfig {
  dbPath: string;
  port: number;
  host: string;
  tlsCertPath: string | null;
  tlsKeyPath: string | null;
}

export const loadServeConfig = (env: NodeJS.ProcessEnv = process.env): ServeConfig => ({
  dbPath: env.DB_PATH ?? "/var/lib/sharepaste/sharepaste.sqlite",
  port: Number(env.PORT ?? 8443),
  host: env.HOST ?? "0.0.0.0",
  tlsCertPath: env.TLS_CERT ?? null,
  tlsKeyPath: env.TLS_KEY ?? null,
});
