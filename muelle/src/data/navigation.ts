import { site } from "@/data/site";

export const navigation = [
  { label: "How", href: "#how" },
  { label: "Features", href: "#features" },
  { label: "Install", href: "#install" },
] as const;

export const footerGroups = [
  {
    title: "Product",
    links: [
      { label: "How it works", href: "#how", external: false },
      { label: "Features", href: "#features", external: false },
      { label: "Install", href: "#install", external: false },
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
