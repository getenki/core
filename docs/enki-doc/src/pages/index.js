import React from "react";
import Head from "@docusaurus/Head";
import styles from "./index.module.css";

export default function Home() {
  return (
    <>
      <Head>
        <title>enki-py documentation</title>
        <meta
          name="description"
          content="Documentation for the enki-py Python bindings and agent wrapper."
        />
      </Head>
      <main className={styles.page}>
        <iframe
          className={styles.frame}
          src="/home/index.html"
          title="enki-py home"
        />
      </main>
    </>
  );
}
