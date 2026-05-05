export type ConnectionState = "Disconnected" | "Connecting" | "Online" | "AuthFailed";

export type EntryView = {
  id: number;
  user_id: string;
  preview: string;
  created_at: number;
  device_id: string;
  device_label?: string;
};

export type Account = {
  user_id: string;
  device_id: string;
  label: string;
  server_url: string;
  status: ConnectionState;
  pending: number;
  is_active: boolean;
};

export type Settings = {
  capture_enabled: boolean;
  deny_list: string[];
  autostart: boolean;
  hotkey?: string | null;
};

export type AppErrorPayload = { kind: string; message: string };
