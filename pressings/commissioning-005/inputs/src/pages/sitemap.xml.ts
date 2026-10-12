import type { APIRoute } from "astro";
import { pressing, site } from "../data";
export const GET: APIRoute = () => {
  const urls = ["/", ...pressing.entries.filter((e) => e.url.endsWith("/")).map((e) => e.url)];
  const body = `<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">${urls.map((url) => `<url><loc>${site}${url}</loc></url>`).join("")}</urlset>`;
  return new Response(body, { headers: { "Content-Type": "application/xml" } });
};
