// eslint-disable-next-line @typescript-eslint/no-require-imports
const enMessages = require('../locales/en/common.json');

function getRequestConfig(config) {
  return config;
}

function getMessages() {
  return Promise.resolve({});
}

function getLocale() {
  return Promise.resolve('en');
}

function resolveMessage(namespace, key) {
  const parts = (namespace ? `${namespace}.${key}` : key).split('.');
  let value = enMessages;
  for (const part of parts) {
    if (value && typeof value === 'object' && Object.prototype.hasOwnProperty.call(value, part)) {
      value = value[part];
    } else {
      return undefined;
    }
  }
  return typeof value === 'string' ? value : undefined;
}

function interpolate(message, values) {
  if (!values) return message;
  return message.replace(/\{(\w+)\}/g, (match, name) =>
    Object.prototype.hasOwnProperty.call(values, name) ? String(values[name]) : match,
  );
}

/**
 * Mirrors next-intl/server's async getTranslations: returns a Promise of a
 * translator that resolves keys against the real English messages so server
 * component tests can assert on readable text. Falls back to the key.
 */
async function getTranslations(namespace) {
  return (key, values) => {
    const message = resolveMessage(namespace, key);
    return message === undefined ? key : interpolate(message, values);
  };
}

module.exports = {
  getRequestConfig,
  getMessages,
  getLocale,
  getTranslations,
};
