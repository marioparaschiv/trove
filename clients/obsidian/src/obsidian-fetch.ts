import { requestUrl } from "obsidian";

const NULL_BODY_STATUSES = new Set([204, 205, 304]);

/**
 * `fetch` over Obsidian's `requestUrl`. The renderer's own `fetch` comes from the app's origin, so
 * CORS blocks it from calling the local service.
 */
export async function obsidianFetch(request: Request): Promise<Response> {
  const headers: Record<string, string> = {};
  request.headers.forEach((value, name) => {
    headers[name] = value;
  });

  const hasBody = request.method !== "GET" && request.method !== "HEAD";
  const response = await requestUrl({
    url: request.url,
    method: request.method,
    headers,
    body: hasBody ? await request.arrayBuffer() : undefined,
    throw: false,
  });

  return new Response(NULL_BODY_STATUSES.has(response.status) ? null : response.arrayBuffer, {
    status: response.status,
    headers: response.headers,
  });
}
