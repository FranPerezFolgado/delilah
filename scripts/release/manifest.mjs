// Builds the update manifest (latest.json) the installed apps read.
export function manifest({ version, notes, pubDate, signature, repo }) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error(`version must be X.Y.Z without a leading "v": ${version}`);
  return {
    version,
    notes,
    pub_date: pubDate,
    platforms: {
      "darwin-aarch64": {
        signature,
        url: `https://github.com/${repo}/releases/download/v${version}/Delilah.app.tar.gz`,
      },
    },
  };
}
