"use client";

import { useState } from "react";

type Props = {
  onSearch: (query: string) => void;
};

export default function SearchBox({ onSearch }: Props) {
  const [query, setQuery] = useState("");

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (query.trim()) {
      onSearch(query);
    }
  };

  return (
    <form onSubmit={handleSubmit} style={{ marginBottom: "1rem" }}>
      <input
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="Enter movie title"
        style={{ padding: "0.5rem", width: "250px" }}
      />
      <button
        type="submit"
        style={{ marginLeft: "0.5rem", padding: "0.5rem 1rem" }}
      >
        Search
      </button>
    </form>
  );
}
