// The app's version (D162): the release number and the git commit it was built from, so a
// report can be traced to the exact code. Vite fills both in (vite.config.ts); tests run without.
import { brand } from './brand';

export const APP_VERSION: string = typeof __APP_VERSION__ === 'string' ? __APP_VERSION__ : '0.0.0';
export const APP_COMMIT: string = typeof __APP_COMMIT__ === 'string' ? __APP_COMMIT__ : 'unknown';

/** What Settings shows: `0.1.0 · 3c70818`. */
export const versionLabel = `${APP_VERSION} · ${APP_COMMIT}`;

/** What a click copies, ready to paste into a bug report: `Cutaway 0.1.0 (3c70818)`. */
export const versionLine = `${brand.displayName} ${APP_VERSION} (${APP_COMMIT})`;
