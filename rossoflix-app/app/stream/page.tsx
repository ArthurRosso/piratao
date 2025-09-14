"use client";

import { useSearchParams } from "next/navigation";

export default function StreamPage() {
  const searchParams = useSearchParams();
  const filename = searchParams.get("filename");
  const magnet = searchParams.get("magnet");

  if (!filename || !magnet) {
    return <p>Missing filename or magnet</p>;
  }

  // URL do backend Rust
  const streamUrl = `http://localhost:8080/stream?filename=${encodeURIComponent(
    filename
  )}&magnet=${encodeURIComponent(magnet)}`;

  return (
    <div>
      <h1>Streaming: {filename}</h1>
      <video
        width={800}
        height={450}
        controls
        autoPlay
        src={streamUrl}
      >
        Your browser does not support the video tag.
      </video>
    </div>
  );
}
