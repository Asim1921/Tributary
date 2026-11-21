import type { Lang } from "../languages";
import { ar } from "./ar";
import { en, type Words } from "./en";
import { es } from "./es";
import { hi } from "./hi";
import { ur } from "./ur";

export type { Words };

export const WORDS: Record<Lang, Words> = { en, ur, ar, hi, es };
