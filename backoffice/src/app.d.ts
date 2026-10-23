declare global {
  interface ImportMetaEnv {
    readonly VITE_CENTRAL_URL?: string;
    readonly VITE_CENTRAL_TOKEN?: string;
  }

  interface ImportMeta {
    readonly env: ImportMetaEnv;
  }
}

export {};
