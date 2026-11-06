import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

const { activate, deactivate, invoke } = await import("../dist/extension.js");
const originalHostCall = globalThis.__continueHostCall;

afterEach(() => {
  if (originalHostCall) globalThis.__continueHostCall = originalHostCall;
  else delete globalThis.__continueHostCall;
});

function installFixtureHost() {
  const calls = [];
  globalThis.__continueHostCall = async (method, params) => {
    calls.push({ method, params });
    if (method === "secret.get") return { value: "fixture-secret" };
    if (method === "kv.set" || method === "log.write") return {};
    if (method === "net.download") {
      return { status: 200, headers: [], finalUrl: params.url, file: params.destination, bytesWritten: 128 };
    }
    if (method === "net.request" && params.url.endsWith("/oauth/token")) {
      return { status: 200, headers: [], finalUrl: params.url, body: { access_token: "fixture-token" } };
    }
    if (method === "net.request" && params.body?.value?.includes("where id =")) {
      return { status: 200, headers: [], finalUrl: params.url, body: [{ id: 42, name: "Fixture Detail", summary: "Offline fixture" }] };
    }
    if (method === "net.request") {
      return { status: 200, headers: [], finalUrl: params.url, body: [{ id: 42, name: "Fixture Game", cover: { url: "https://images.example.test/cover/42.jpg" } }] };
    }
    throw new Error(`Unexpected fixture host method: ${method}`);
  };
  return calls;
}

test("exports the Runtime module contract and runs search without network access", async () => {
  const calls = installFixtureHost();
  assert.equal(typeof activate, "function");
  assert.equal(typeof invoke, "function");
  assert.equal(typeof deactivate, "function");

  const result = await invoke("igdb.search", { query: "fixture game", limit: 3 });
  assert.deepEqual(result, {
    candidates: [{ externalId: "42", title: "Fixture Game", coverUrl: "https://images.example.test/cover/42.jpg" }],
  });
  assert.deepEqual(calls.map((call) => call.method), [
    "secret.get",
    "net.request",
    "kv.set",
    "net.request",
    "log.write",
  ]);
  assert.equal(calls[1].params.url, "https://api.example.test/oauth/token");
  assert.equal(calls[3].params.url, "https://api.example.test/v4/games");
});

test("runs scrape through detail request and FileRef download", async () => {
  const calls = installFixtureHost();
  const result = await invoke("igdb.scrape", { externalId: "42" });

  assert.equal(result.source.provider, "igdb-like");
  assert.equal(result.source.externalId, "42");
  assert.equal(result.game.title, "Fixture Detail");
  assert.deepEqual(result.media, [{
    kind: "box_art",
    sourceUrl: "https://images.example.test/cover/42.jpg",
    file: { mount: "extension-cache", path: "igdb-like/covers/42.jpg" },
  }]);
  assert.deepEqual(result.externalIds, { igdb: "42" });
  const download = calls.find((call) => call.method === "net.download");
  assert.ok(download);
  assert.deepEqual(download.params.destination, { mount: "extension-cache", path: "igdb-like/covers/42.jpg" });
  assert.deepEqual(calls.map((call) => call.method), [
    "secret.get",
    "net.request",
    "kv.set",
    "net.request",
    "net.download",
  ]);
  assert.equal(calls.filter((call) => call.method === "secret.get").length, 1);
});
