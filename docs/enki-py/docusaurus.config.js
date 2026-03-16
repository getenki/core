const config = {
  title: "enki-py",
  tagline: "Python bindings and agent wrapper for Enki",
  favicon: "img/favicon.svg",

  url: "http://localhost",
  baseUrl: "/",

  organizationName: "enki",
  projectName: "enki-py-docs",

  onBrokenLinks: "throw",
  i18n: {
    defaultLocale: "en",
    locales: ["en"]
  },
  markdown: {
    hooks: {
      onBrokenMarkdownLinks: "warn"
    }
  },

  presets: [
    [
      "classic",
      {
        docs: {
          sidebarPath: require.resolve("./sidebars.js"),
          routeBasePath: "docs"
        },
        blog: false,
        theme: {
          customCss: require.resolve("./src/css/custom.css")
        }
      }
    ]
  ],

  themeConfig: {
    colorMode: {
      defaultMode: "light",
      disableSwitch: false,
      respectPrefersColorScheme: true
    },
    navbar: {
      title: "enki-py",
      items: [
        {
          type: "docSidebar",
          sidebarId: "docsSidebar",
          position: "left",
          label: "Documentation"
        }
      ]
    },
    footer: {
      style: "dark",
      links: [
        {
          title: "Docs",
          items: [
            {
              label: "Getting Started",
              to: "/docs/intro"
            },
            {
              label: "Agent Wrapper",
              to: "/docs/agent-wrapper"
            }
          ]
        },
        {
          title: "Source",
          items: [
            {
              label: "Rust crate",
              to: "/docs/low-level-api"
            },
            {
              label: "Examples",
              to: "/docs/examples"
            }
          ]
        }
      ],
      copyright: `Copyright ${new Date().getFullYear()} Enki`
    },
    prism: {
      additionalLanguages: ["python", "rust", "toml"]
    }
  },
  themes: []
};

module.exports = config;
