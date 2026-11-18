function useRouter() {
  return {
    push: jest.fn(),
    replace: jest.fn(),
    prefetch: jest.fn(),
    back: jest.fn(),
    forward: jest.fn(),
    refresh: jest.fn(),
  };
}

function usePathname() {
  return '/';
}

function useSearchParams() {
  return new URLSearchParams();
}

function useParams() {
  return {};
}

module.exports = {
  useRouter,
  usePathname,
  useSearchParams,
  useParams,
};
