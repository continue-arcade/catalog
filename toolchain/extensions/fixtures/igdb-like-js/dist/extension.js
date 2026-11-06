import { sdk } from "@continue/sdk";

let apiRoot = "https://api.example.test";
let imageRoot = "https://images.example.test";
const SECRET_NAME = "igdb-like-client-secret";
const TOKEN_KEY = "igdb-like.access-token";
const PROVIDER = "igdb-like";

export async function activate(context = {}) {
  if (typeof context.apiRoot === "string" && context.apiRoot.length > 0) {
    apiRoot = context.apiRoot.replace(/\/$/, "");
  }
  if (typeof context.imageRoot === "string" && context.imageRoot.length > 0) {
    imageRoot = context.imageRoot.replace(/\/$/, "");
  }
  await sdk.log.write({
    level: "info",
    message: "IGDB-like JavaScript fixture activated",
    fields: { runtime: context.runtime ?? "quickjs" },
  });
}

export async function invoke(command, params = {}) {
  if (command === "igdb.search") {
    return search(params);
  }
  if (command === "igdb.scrape") {
    return scrape(params);
  }
  throw new Error(`Unsupported fixture command: ${command}`);
}

export async function deactivate() {
  await sdk.log.write({
    level: "debug",
    message: "IGDB-like JavaScript fixture deactivated",
  });
}

async function search(params) {
  const query = requireQuery(params);
  const limit = normalizeLimit(params.limit);
  const accessToken = await authenticate();
  return searchWithToken(query, limit, accessToken);
}

async function searchWithToken(query, limit, accessToken) {
  const response = await sdk.net.request({
    method: "POST",
    url: `${apiRoot}/v4/games`,
    headers: [
      { name: "Authorization", value: `Bearer ${accessToken}` },
      { name: "Content-Type", value: "text/plain" },
    ],
    body: {
      type: "text",
      value: `search "${escapeQuery(query)}"; fields id,name,cover.url; limit ${limit};`,
      contentType: "text/plain",
    },
    responseMode: "json",
  });

  const candidates = extractGames(response.body)
    .slice(0, limit)
    .map((game, index) => toCandidate(game, index));

  await sdk.log.write({
    level: "debug",
    message: "IGDB-like search completed",
    fields: { resultCount: candidates.length, queryLength: query.length },
  });

  return { candidates };
}

async function scrape(params) {
  const externalId = requireExternalId(params);
  const accessToken = await authenticate();
  const detailResponse = await sdk.net.request({
    method: "POST",
    url: `${apiRoot}/v4/games`,
    headers: [
      { name: "Authorization", value: `Bearer ${accessToken}` },
      { name: "Content-Type", value: "text/plain" },
    ],
    body: {
      type: "text",
      value: `fields id,name,summary,cover.url; where id = ${escapeNumericId(externalId)};`,
      contentType: "text/plain",
    },
    responseMode: "json",
  });
  const details = extractGames(detailResponse.body)[0] ?? {};
  const coverUrl = getCoverUrl(details) ?? `${imageRoot}/cover/${encodeURIComponent(externalId)}.jpg`;
  const media = [];

  if (params.downloadCover !== false && coverUrl) {
    const cacheFile = {
      mount: "extension-cache",
      path: `igdb-like/covers/${externalId}.jpg`,
    };
    const download = await sdk.net.download({
      method: "GET",
      url: coverUrl,
      headers: [{ name: "Authorization", value: `Bearer ${accessToken}` }],
      destination: cacheFile,
      overwrite: true,
    });
    media.push({
      kind: "box_art",
      sourceUrl: coverUrl,
      file: download.file ?? cacheFile,
    });
  }

  return {
    schemaVersion: "0.1",
    source: {
      provider: PROVIDER,
      externalId,
      observedAt: new Date().toISOString(),
    },
    game: {
      title: details.name ?? `Fixture game ${externalId}`,
      summary: details.summary ?? null,
      releaseDate: null,
      genres: [],
      companies: [],
      sourcePlatforms: [],
    },
    media,
    externalIds: { igdb: externalId },
  };
}

async function authenticate() {
  const secret = await sdk.secret.get({ name: SECRET_NAME });
  const tokenResponse = await sdk.net.request({
    method: "POST",
    url: `${apiRoot}/oauth/token`,
    headers: [{ name: "Content-Type", value: "application/x-www-form-urlencoded" }],
    body: {
      type: "text",
      value: `client_secret=${encodeURIComponent(secret.value)}&grant_type=client_credentials`,
      contentType: "application/x-www-form-urlencoded",
    },
    responseMode: "json",
  });
  const token = tokenResponse.body?.access_token;
  if (typeof token !== "string" || token.length === 0) {
    throw new Error("Fixture token response did not contain access_token");
  }
  await sdk.kv.set({ key: TOKEN_KEY, value: { accessToken: token } });
  return token;
}

function requireQuery(params) {
  const query = params?.query;
  if (typeof query !== "string" || query.trim().length === 0) {
    throw new Error("query must not be empty");
  }
  return query.trim();
}

function normalizeLimit(value) {
  if (value === undefined) return 10;
  if (!Number.isInteger(value) || value < 1 || value > 10) {
    throw new Error("limit must be an integer between 1 and 10");
  }
  return value;
}

function extractGames(body) {
  if (Array.isArray(body)) return body;
  if (body && Array.isArray(body.games)) return body.games;
  return body && typeof body === "object" ? [body] : [];
}

function toCandidate(game, index) {
  const igdbId = String(game.id ?? game.igdb_id ?? `fixture-${index + 1}`);
  return {
    externalId: igdbId,
    title: typeof game.name === "string" ? game.name : `Fixture game ${igdbId}`,
    coverUrl: getCoverUrl(game),
  };
}

function getCoverUrl(game) {
  if (typeof game?.cover_url === "string") return game.cover_url;
  if (typeof game?.cover?.url === "string") return game.cover.url;
  return null;
}

function requireExternalId(params) {
  const externalId = params?.externalId;
  if (typeof externalId !== "string" || externalId.trim().length === 0) {
    throw new Error("externalId must not be empty");
  }
  return externalId.trim();
}

function escapeNumericId(value) {
  if (!/^\d+$/.test(value)) {
    throw new Error("externalId must contain decimal digits in this fixture");
  }
  return value;
}

function escapeQuery(value) {
  return value.replaceAll('"', '\\"');
}
