"use client";

import { useState } from "react";
import MovieList from "../components/MovieList";
import SearchBox from "../components/SearchBox";

export default function HomePage() {
  const [movies, setMovies] = useState<any[]>([]);

  const handleSearch = async (query: string) => {
    const res = await fetch(
      `http://localhost:8080/search?q=${encodeURIComponent(query)}`,
    );
    const data = await res.json();
    setMovies(data.results || []);
  };

  return (
    <div>
      <SearchBox onSearch={handleSearch} />
      <MovieList movies={movies} />
    </div>
  );
}
