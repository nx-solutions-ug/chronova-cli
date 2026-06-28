module.exports = {
  branches: ["main"],
  tagFormat: "v.${version}",
  repositoryUrl: "https://github.com/nx-solutions-ug/chronova-cli.git",
  plugins: [
    "@semantic-release/commit-analyzer",
    "@semantic-release/release-notes-generator",
    "@semantic-release/github",
  ],
};
