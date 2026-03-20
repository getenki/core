const config = {
  title: "Enki Docs",
  tagline: "Rust runtime plus Python and JavaScript bindings for Enki",
  favicon: "img/favicon.png",

  url: "http://localhost",
  baseUrl: "/",

  organizationName: "getenki",
  projectName: "enki-docs",

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
      title: "Enki Docs",
      logo: {
        alt: "Enki logo",
        src: "img/logo-light.png",
        srcDark: "img/logo-dark.png"
      },
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
              label: "Python Quickstart",
              to: "/docs/agent-wrapper"
            }
          ]
        },
        {
          title: "Source",
          items: [
            {
              label: "JavaScript",
              to: "/docs/javascript"
            },
            {
              label: "Rust",
              to: "/docs/rust"
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
