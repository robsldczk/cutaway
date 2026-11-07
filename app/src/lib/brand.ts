// Product names come from brand.json at the repo root (see docs/specs/architecture.md, Naming).
import brandJson from '../../../brand.json';

export interface Brand {
  displayName: string;
  slug: string;
  cliName: string;
  episodeExt: string;
  showExt: string;
  reviewExt: string;
  bundleId: string;
  tagline: string;
  /** Who the app belongs to (the imprint, the bundle's publisher). */
  owner: string;
  /** The copyright line: Settings > Licenses, the About box, the bundle. */
  copyright: string;
}

export const brand: Brand = brandJson;
