const manifestCache = new Map();
const bundleCache = new Map();
const toLanguageRoot = (languageId) => `/tree-sitter/languages/${languageId}/`;

export const fetchLanguageManifest = async (languageId) => {
  if (!languageId) {
    throw new Error('languageId is required');
  }

  if (manifestCache.has(languageId)) {
    return manifestCache.get(languageId);
  }

  const loader = (async () => {
    const response = await fetch(`${toLanguageRoot(languageId)}editor/asset-manifest.json`);
    if (!response.ok) {
      throw new Error(`Failed to load asset manifest for ${languageId}`);
    }
    return response.json();
  })();

  manifestCache.set(
    languageId,
    loader.catch((error) => {
      manifestCache.delete(languageId);
      throw error;
    }),
  );
  return manifestCache.get(languageId);
};

export const resolveLanguageAssetUrl = (languageId, relativePath) => {
  if (!relativePath) {
    return null;
  }
  return new URL(relativePath, window.location.origin + toLanguageRoot(languageId)).toString();
};

export const fetchCompletionBundle = async (languageId) => {
  if (!languageId) {
    return null;
  }

  if (bundleCache.has(languageId)) {
    return bundleCache.get(languageId);
  }

  const loader = (async () => {
    const manifest = await fetchLanguageManifest(languageId);
    if (!manifest?.completion_bundle) {
      return null;
    }

    const response = await fetch(resolveLanguageAssetUrl(languageId, manifest.completion_bundle));
    if (!response.ok) {
      throw new Error(`Failed to load completion bundle for ${languageId}`);
    }
    return response.json();
  })();

  bundleCache.set(
    languageId,
    loader.catch((error) => {
      bundleCache.delete(languageId);
      throw error;
    }),
  );
  return bundleCache.get(languageId);
};
