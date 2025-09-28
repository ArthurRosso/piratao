// "use client";

// import { useState } from "react";
// import MovieList from "../components/MovieList";
// import SearchBox from "../components/SearchBox";

// export default function HomePage() {
//   const [movies, setMovies] = useState<any[]>([]);

//   const handleSearch = async (query: string) => {
//     const res = await fetch(
//       `http://localhost:8080/search?q=${encodeURIComponent(query)}`,
//     );
//     const data = await res.json();
//     setMovies(data.results || []);
//   };

//   return (
//     <div>
//       <SearchBox onSearch={handleSearch} />
//       <MovieList movies={movies} />
//     </div>
//   );
// }
"use client";

import { useEffect, useState } from "react";
import MovieList from "../components/MovieList";
import SearchBox from "../components/SearchBox";

export default function HomePage() {
  const [movies, setMovies] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);

  const handleSearch = async (query: string) => {
    setLoading(true);
    const res = await fetch(
      `http://localhost:8080/search?q=${encodeURIComponent(query)}`,
    );
    const data = await res.json();
    setMovies(data.results || []);
    setLoading(false);
  };

  // Fetch trending movies on mount
  useEffect(() => {
    const fetchTrending = async () => {
      try {
        const res = await fetch("http://localhost:8080/movies/trending");
        const data = await res.json();
        setMovies(data.results || []);
      } catch (err) {
        console.error("Failed to load trending movies", err);
      } finally {
        setLoading(false);
      }
    };
    fetchTrending();
  }, []);

  return (
    <div>
      <SearchBox onSearch={handleSearch} />
      {loading ? <p>Loading...</p> : <MovieList movies={movies} />}
    </div>
  );
}
