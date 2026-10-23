import { snippetCompletion } from '@codemirror/autocomplete';
import { fetchCompletionBundle } from './assetRegistry';

const optionsCache = new Map();
const regexCache = new Map();

export const buildCompletionOptionsFromBundle = (bundle, lang) => {
  const items = bundle?.locales?.[lang] || bundle?.locales?.['zh-CN'] || [];
  return items.map((item) =>
    snippetCompletion(item.insert_text, {
      label: item.label,
      type: item.type,
      detail: item.detail,
      info: item.info,
    }),
  );
};

const getValidFor = (languageId, bundle) => {
  const cacheKey = `${languageId}:${bundle?.valid_for || ''}`;
  if (regexCache.has(cacheKey)) {
    return regexCache.get(cacheKey);
  }

  const validFor = new RegExp(bundle?.valid_for || '[\\w/:\\[\\]]+|\\|');
  regexCache.set(cacheKey, validFor);
  return validFor;
};

const getCompletionOptions = async (languageId, lang) => {
  const cacheKey = `${languageId}:${lang}`;
  if (optionsCache.has(cacheKey)) {
    return optionsCache.get(cacheKey);
  }

  const loader = (async () => {
    const bundle = await fetchCompletionBundle(languageId);
    if (!bundle) {
      return { bundle: null, options: [] };
    }

    return {
      bundle,
      options: buildCompletionOptionsFromBundle(bundle, lang),
    };
  })();

  optionsCache.set(cacheKey, loader);
  return loader;
};

export const createBundleCompletionSource = (languageId, lang) => async (context) => {
  const { bundle, options } = await getCompletionOptions(languageId, lang);
  if (!bundle) {
    return null;
  }

  const validFor = getValidFor(languageId, bundle);
  const word = context.matchBefore(validFor);
  const pipe = context.matchBefore(/\|/);

  if (!word && !pipe && !context.explicit) {
    return null;
  }

  return {
    from: (pipe || word)?.from ?? context.pos,
    options,
    validFor,
  };
};
