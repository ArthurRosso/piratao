import "./globals.css";
import Link from "next/link";
import { ReactNode } from "react";

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en">
      <body>
        <header
          style={{ background: "#20232a", color: "#61dafb", padding: "1rem" }}
        >
          <nav>
            <Link href="/">🎬 Rossoflix</Link>
          </nav>
        </header>
        <main style={{ padding: "1rem" }}>{children}</main>
      </body>
    </html>
  );
}
