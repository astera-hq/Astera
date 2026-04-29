FROM node:20-alpine

WORKDIR /app

# Install dependencies first for better caching
COPY package*.json ./
RUN npm install

# Copy source
COPY . .

# We don't necessarily need to build if we use tsx in dev, 
# but for production-ready skeleton we include it.
RUN npm run build

CMD ["npm", "start"]
