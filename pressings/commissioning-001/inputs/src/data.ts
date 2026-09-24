import { readFileSync } from "node:fs";
import { resolve } from "node:path";

export interface PressedEntry {
  url: string;
  sha256: string;
  bytes: number;
  title: string | null;
  description: string | null;
  date: string | null;
}
export interface Pressing {
  schema: number;
  release: string;
  source_tree_sha256: string;
  entries: PressedEntry[];
}
// A site build requires a real pressing. There is no fabricated fallback state.
export const pressing = JSON.parse(
  readFileSync(resolve(process.cwd(), "public/pressing.json"), "utf8"),
) as Pressing;
export const works = pressing.entries.filter((entry) => entry.url !== "/published/" && entry.url.endsWith("/"));
export const site = "https://studio.mazzeleczzare.com";
