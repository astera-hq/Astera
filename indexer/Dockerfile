FROM node:26-alpine

WORKDIR /indexer

COPY package*.json ./
RUN npm install --production

COPY . .
RUN npm run build

EXPOSE 3001 9091

CMD ["node", "dist/index.js"]
