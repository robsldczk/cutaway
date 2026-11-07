// Settings > Imprint and Settings > Licenses (D161). The imprint is Rob's, as published at
// https://robertsladeczek.com/imprint/ (§ 5 DDG): English first, then the German original.
import { brand } from './brand';

export interface ImprintSection {
  heading: string;
  lines: string[];
}

export interface ImprintBlock {
  language: string;
  sections: ImprintSection[];
}

const address = ['Robert Sladeczek', 'c/o Autorenglück #16657', 'Albert-Einstein-Str. 47', '02977 Hoyerswerda'];
/** Written as on the website. */
const email = 'sladeczek.robert[at]gmail.com';

export const IMPRINT: ImprintBlock[] = [
  {
    language: 'English',
    sections: [
      { heading: 'Legal notice (according to § 5 DDG)', lines: [...address, 'Germany'] },
      { heading: 'Contact', lines: [`Email: ${email}`] },
    ],
  },
  {
    language: 'Deutsch',
    sections: [
      { heading: 'Angaben gemäß § 5 DDG', lines: [...address, 'Deutschland'] },
      { heading: 'Kontakt', lines: [`E-Mail: ${email}`] },
    ],
  },
];

export const IMPRINT_SOURCE = 'https://robertsladeczek.com/imprint/';

export const LEGAL = {
  fullName: brand.displayName,
  copyright: brand.copyright,
  proprietary: `${brand.displayName} is proprietary software. It includes the following work by others, each under its own license.`,
};

// ---- the license list (scripts/licenses.mjs writes it) ----

export interface LicensedPackage {
  /** `npm` and `rust` are packages; `other` is anything else the app uses (the speech model). */
  kind: 'npm' | 'rust' | 'other';
  name: string;
  version: string;
  /** As the package states it, e.g. "MIT OR Apache-2.0". */
  declared: string;
  /** What Cutaway uses it under, e.g. "MIT". */
  license: string;
  repository?: string;
  authors?: string;
  note?: string;
  /** Keys into `LicenseList.texts`. */
  texts: string[];
}

export interface LicenseList {
  packages: LicensedPackage[];
  /** Each license text once, by a short hash of it. */
  texts: Record<string, string>;
}

/** Packages whose name, license or author has every word of the query. */
export function matching(list: LicensedPackage[], query: string): LicensedPackage[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return list;
  return list.filter((p) => {
    const hay = `${p.name} ${p.license} ${p.declared} ${p.authors ?? ''}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}
