import { site } from "@/data/site";

export const navigation = [
  { label: "Dock", href: "#dock" },
  { label: "Settings", href: "#customize" },
  { label: "Reversible", href: "#safety" },
] as const;

export const footerGroups = [
  {
    title: "Product",
    links: [
      { label: "The Dock", href: "#dock", external: false },
      { label: "Settings", href: "#customize", external: false },
      { label: "Reversible", href: "#safety", external: false },
      { label: "Download", href: "#download", external: false },
    ],
  },
  {
    title: "Source",
    links: [
      { label: "GitHub", href: site.repository, external: true },
      { label: "Releases", href: site.releases, external: true },
      { label: "Issues", href: site.issues, external: true },
    ],
  },
  {
    title: "Project",
    links: [
      { label: "License", href: `${site.repository}/blob/main/LICENSE`, external: true },
      { label: "Changelog", href: site.releases, external: true },
      { label: "Contribute", href: site.issues, external: true },
    ],
  },
] as const;
