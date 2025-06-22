// @ts-check
import { defineConfig } from 'astro/config';

function getSiteUrl() {
    if (!process.env.SITE_URL) {
        throw new Error('SITE_URL is not set');
    }
    return process.env.SITE_URL;
}

// https://astro.build/config
export default defineConfig({
  site: getSiteUrl(),
  integrations: [],
});
