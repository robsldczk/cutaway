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
}

export const brand: Brand = brandJson;
