export type ModelSpec = { providerID: string; modelID: string; variant?: string };

// Config files and the /subagents state file spell models as provider/model[#variant].
const MODEL_PATTERN = /^([^/\s#]+)\/([^\s#]+)(?:#([^\s#]+))?$/;

export const formatModel = ({ providerID, modelID, variant }: ModelSpec) =>
  `${providerID}/${modelID}${variant === undefined ? "" : `#${variant}`}`;

export const parseModel = (value: string): ModelSpec | undefined => {
  const match = MODEL_PATTERN.exec(value.trim());
  if (match === null) return undefined;
  return match[3] === undefined
    ? { providerID: match[1], modelID: match[2] }
    : { providerID: match[1], modelID: match[2], variant: match[3] };
};

export const asModelSpec = (value: unknown): ModelSpec | undefined => {
  if (typeof value !== "object" || value === null) return undefined;
  const { providerID, modelID, variant } = value as Record<string, unknown>;
  if (typeof providerID !== "string" || providerID.length === 0) return undefined;
  if (typeof modelID !== "string" || modelID.length === 0) return undefined;
  if (variant !== undefined && typeof variant !== "string") return undefined;
  return variant === undefined ? { providerID, modelID } : { providerID, modelID, variant };
};

// Prompt cache is keyed by provider/model; variant-only differences (e.g.
// sol-fast high vs low) keep the same model. Used to pin a child session to
// the model of its first turn.
export const sameModel = (a: ModelSpec, b: ModelSpec) =>
  a.providerID === b.providerID && a.modelID === b.modelID;
