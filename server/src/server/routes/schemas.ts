export const UUID = {
  type: "string",
  pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$",
} as const;

export const HEX_64 = { type: "string", pattern: "^[0-9a-fA-F]{64}$" } as const;

export const SECRET_PROOF = { type: "string", minLength: 16, maxLength: 256 } as const;

export const DEVICE_LABEL = { type: "string", minLength: 1, maxLength: 128 } as const;
