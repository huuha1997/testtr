import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Agentic",
  description: "AI-powered design-to-deploy pipeline",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en" className="dark">
      <body className="min-h-screen bg-bg-primary text-text-primary antialiased">
        {children}
      </body>
    </html>
  );
}
