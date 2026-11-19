FROM node:20-alpine AS runtime

WORKDIR /workspace

COPY packages/sdk /workspace/packages/sdk
WORKDIR /workspace/packages/sdk
RUN npm install && npm run build

COPY oracle-service/package.json /workspace/oracle-service/package.json
COPY oracle-service/tsconfig.json /workspace/oracle-service/tsconfig.json
COPY oracle-service/src /workspace/oracle-service/src

WORKDIR /workspace/oracle-service
RUN npm install && npm run build

HEALTHCHECK --interval=10s --timeout=5s --start-period=15s --retries=3 \
  CMD node -e "fetch(\`http://127.0.0.1:${HEALTH_PORT:-8080}/health\`).then((res) => process.exit(res.ok ? 0 : 1)).catch(() => process.exit(1))"

CMD ["npm", "start"]
