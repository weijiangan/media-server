export const API_HOST = "localhost";
export const API_PORT = 8080;
export const API_BASE_URL = process.env.NODE_ENV === 'production' ? location.origin : `http://${API_HOST}:${API_PORT}`;
