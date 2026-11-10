export type CreditStatus = {
  route: "promotional" | "subscription";
  organization_id: string;
};

export function parseCreditStatus(value: unknown, organization: string): CreditStatus {
  if (
    !value || typeof value !== "object" ||
    !("route" in value) || !["promotional", "subscription"].includes(String(value.route)) ||
    !("organization_id" in value) || value.organization_id !== organization
  ) throw new Error("Claude credit router returned an invalid status or a different organization");
  return value as CreditStatus;
}

export function creditEndpoint(settings: Record<string, unknown>, upstream: string, router: string): string {
  if (settings.baseURL !== upstream || settings.apiKey !== undefined || settings.authToken !== undefined) {
    throw new Error("Claude credit routing requires the existing Sleev subscription endpoint and native Claude login; leaving provider unchanged");
  }
  return router;
}
