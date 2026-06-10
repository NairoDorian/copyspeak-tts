
let mockPathname = "/";

export const page = {
  get url() {
    return { pathname: mockPathname };
  }
};

if (typeof global !== "undefined") {
  (global as any).__setMockPathname = (pathname: string) => {
    mockPathname = pathname;
  };
}
