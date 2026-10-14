type Server = {
  type: string;
  url?: string;
  disabled?: boolean;
  headers?: Record<string, string>;
};
export function applyCredentials(server: Server, key?: string) {
  if (
    server.type !== "remote" ||
    server.disabled ||
    !server.url?.startsWith("https://backend.composio.dev/tool_router/")
  )
    return;
  if (key) server.headers = { ...server.headers, "x-user-api-key": key };
  else server.disabled = true;
}
