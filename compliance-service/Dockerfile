FROM node:20-alpine

WORKDIR /app

COPY package*.json ./
RUN npm install

COPY . .
RUN npm run build

# Mirrors the healthcheck already declared for this service in
# docker-compose.yml, so the container reports a health status even when
# run outside compose (e.g. `docker run`, other orchestrators).
HEALTHCHECK --interval=10s --timeout=5s --start-period=15s --retries=3 \
  CMD node -e "fetch(\`http://127.0.0.1:${HEALTH_PORT:-8081}/health\`).then((res) => process.exit(res.ok ? 0 : 1)).catch(() => process.exit(1))"

CMD ["npm", "start"]
