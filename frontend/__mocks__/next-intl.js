// eslint-disable-next-line @typescript-eslint/no-require-imports
const React = require('react');

function useTranslations() {
  return (key) => key;
}

function useLocale() {
  return 'en';
}

function useNow() {
  return new Date();
}

function useTimeZone() {
  return 'UTC';
}

function useMessages() {
  return {};
}

function NextIntlClientProvider({ children }) {
  return React.createElement(React.Fragment, null, children);
}

function IntlProvider({ children }) {
  return React.createElement(React.Fragment, null, children);
}

module.exports = {
  useTranslations,
  useLocale,
  useNow,
  useTimeZone,
  useMessages,
  NextIntlClientProvider,
  IntlProvider,
};
