export type ModelSpec = { providerID: string; modelID: string; variant?: string };

// Config files and the /subagents state file spell models as provider/model[#variant].
const MODEL_PATTERN = /^([^/\s#]+)\/([^\s#]+)(?:#([^\s#]+))?$/;

const modelSpec = (providerID: string, modelID: string, variant?: string): ModelSpec =>
  variant === undefined ? { providerID, modelID } : { providerID, modelID, variant };

export const formatModel = ({ providerID, modelID, variant }: ModelSpec) =>
  `${providerID}/${modelID}${variant === undefined ? "" : `#${variant}`}`;

export const parseModel = (value: string): ModelSpec | undefined => {
  const match = MODEL_PATTERN.exec(value.trim());
  return match === null ? undefined : modelSpec(match[1], match[2], match[3]);
};

export const asModelSpec = (value: unknown): ModelSpec | undefined => {
  if (typeof value !== "object" || value === null) return undefined;
  const { providerID, modelID, variant } = value as Record<string, unknown>;
  if (typeof providerID !== "string" || providerID.length === 0) return undefined;
  if (typeof modelID !== "string" || modelID.length === 0) return undefined;
  if (variant !== undefined && typeof variant !== "string") return undefined;
  return modelSpec(providerID, modelID, variant);
};
