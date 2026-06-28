module.exports = {
  branches: ["main"],
  repositoryUrl: "https://github.com/nx-solutions-ug/chronova-cli.git",
  plugins: [
    "@semantic-release/commit-analyzer",
    "@semantic-release/release-notes-generator",
    "@semantic-release/github",
  ],
};
