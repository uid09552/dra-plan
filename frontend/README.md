# dra-workflow UI

Angular 21 frontend of [dra-workflow](../README.md). Architecture and conventions:
[docs/architecture/frontend.md](../docs/architecture/frontend.md).

```bash
npm ci            # or: make ui-install
npm start         # dev server on http://localhost:4200 (proxies /api and /mcp to 127.0.0.1:8090)
npm test          # unit tests (Vitest)
npm run build     # production build in dist/
```

Licensed under the Apache License, Version 2.0 (see [LICENSE](../LICENSE)).
