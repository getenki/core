const sidebars = {
    docsSidebar: [
        "intro",
        "builder-cli",
        "agent-design",
        "workflow-design",
        {
            type: "category",
            label: "Python",
            items: [
                "python",
                "python-workflow",
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
                "javascript-workflow",
                "javascript-multi-agent"
            ]
        },
        {
            type: "category",
            label: "TypeScript",
            items: [
                "typescript",
                "typescript-workflow",
                "typescript-multi-agent"
            ]
        },
        {
            type: "category",
            label: "Rust",
            items: [
                "rust",
                "rust-workflow",
                "build-from-source"
            ]
        }
    ]
};

module.exports = sidebars;