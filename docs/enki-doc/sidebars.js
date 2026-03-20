const sidebars = {
    docsSidebar: [
        "intro",
        {
            type: "category",
            label: "Python",
            items: [
                "python",
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
                "typescript"
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
