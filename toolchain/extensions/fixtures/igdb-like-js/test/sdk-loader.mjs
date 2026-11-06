export async function resolve(specifier, context, nextResolve) {
  if (specifier === "@continue/sdk") {
    return {
      shortCircuit: true,
      url: new URL("../../../../packages/extension-sdk-js/src/index.ts", import.meta.url).href,
    };
  }
  if (specifier === "./scraper" && context.parentURL?.endsWith("/extension-sdk-js/src/index.ts")) {
    return { shortCircuit: true, url: new URL("./scraper.ts", context.parentURL).href };
  }
  return nextResolve(specifier, context);
}
