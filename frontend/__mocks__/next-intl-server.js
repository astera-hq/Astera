function getRequestConfig(config) {
  return config;
}

function getMessages() {
  return Promise.resolve({});
}

function getLocale() {
  return Promise.resolve('en');
}

module.exports = {
  getRequestConfig,
  getMessages,
  getLocale,
};
