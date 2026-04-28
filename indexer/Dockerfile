FROM node:20-alpine

WORKDIR /indexer

COPY package*.json ./
RUN npm install --production

COPY . .
RUN npm run build

EXPOSE 3001

CMD ["node", "dist/index.js"]
