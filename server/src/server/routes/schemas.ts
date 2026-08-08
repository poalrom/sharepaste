/**
 * The wire fragments a malformed request is refused against. They are declared by the
 * Refusal module (`../refusal.ts`), which owns what makes an act refusable and the status
 * that says so; this path stays because every route already imports them from here, and
 * moving six imports pays no reader back.
 */
export { DEVICE_LABEL, HEX_64, SECRET_PROOF, UUID } from "../refusal.js";
