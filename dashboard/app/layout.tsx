import type { Metadata } from "next";
import type { ReactNode } from "react";

import "@xyflow/react/dist/style.css";
import "./styles.css";
import { Providers } from "./providers";

export const metadata: Metadata = {
  title: "AlethOS ToolPassport",
  description: "Verifiable AI tool audit workspace",
};

export default function RootLayout({ children }: Readonly<{ children: ReactNode }>) {
  return (
    <html lang="en">
      <body>
        <Providers>{children}</Providers>
      </body>
    </html>
  );
}
