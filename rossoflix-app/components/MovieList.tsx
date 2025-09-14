import Link from "next/link";

type Props = {
  movies: any[];
};

export default function MovieList({ movies }: Props) {
  if (movies.length === 0) {
    return <p>No results.</p>;
  }

  return (
    <ul>
      {movies.map((movie) => (
        <li key={movie.imdbID}>
          <Link href={`/movie/${movie.imdbID}`}>
            {movie.Title} ({movie.Year})
          </Link>
        </li>
      ))}
    </ul>
  );
}
