FROM node:20-alpine AS runtime

WORKDIR /workspace

COPY packages/sdk /workspace/packages/sdk
WORKDIR /workspace/packages/sdk
RUN npm install && npm run build

COPY compliance-service/package.json /workspace/compliance-service/package.json
COPY compliance-service/tsconfig.json /workspace/compliance-service/tsconfig.json
COPY compliance-service/src /workspace/compliance-service/src

WORKDIR /workspace/compliance-service
RUN npm install && npm run build

HEALTHCHECK --interval=10s --timeout=5s --start-period=15s --retries=3 \
  CMD node -e "fetch(\`http://127.0.0.1:${HEALTH_PORT:-8081}/health\`).then((res) => process.exit(res.ok ? 0 : 1)).catch(() => process.exit(1))"

CMD ["npm", "start"]
