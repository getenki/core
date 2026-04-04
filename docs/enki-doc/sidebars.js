const sidebars = {
    docsSidebar: [
        "intro",
        "builder-cli",
        "agent-design",
        {
            type: "category",
            label: "Python",
            items: [
                "python",
                "python-multi-agent",
                "installation",
                "agent-wrapper",
                "memory-backends",
                "memory-examples",
                "low-level-api",
                "examples",
                "faq"
            ]
        },
        {
            type: "category",
            label: "JavaScript",
            items: [
                "javascript",
                "javascript-multi-agent"
            ]
        },
        {
            type: "category",
            label: "TypeScript",
            items: [
                "typescript",
                "typescript-multi-agent"
            ]
        },
        {
            type: "category",
            label: "Rust",
            items: [
                "rust",
                "build-from-source"
            ]
        }
    ]
};

module.exports = sidebars;
