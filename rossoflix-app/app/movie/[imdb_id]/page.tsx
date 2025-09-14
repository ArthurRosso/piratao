import { notFound } from "next/navigation";

type Props = {
  params: { imdb_id: string };
};

export default async function MovieDetailPage({ params }: Props) {
  // Buscar detalhes do filme
  const res = await fetch(`http://localhost:8080/movie/${params.imdb_id}`, {
    cache: "no-store",
  });
  if (!res.ok) return notFound();
  const movie = await res.json();

  // Buscar torrents no Torrentio
  const torrentsRes = await fetch(
    `http://localhost:8080/torrentio/movie/${params.imdb_id}`,
    { cache: "no-store" },
  );
  let torrents: any[] = [];
  if (torrentsRes.ok) {
    const json = await torrentsRes.json();
    torrents = json.streams || [];
  }

  return (
    <div>
      <h1>{movie.Title}</h1>
      <p>
        <b>Year:</b> {movie.Year}
      </p>
      <p>
        <b>Genre:</b> {movie.Genre}
      </p>
      <p>
        <b>Plot:</b> {movie.Plot}
      </p>
      {movie.Poster && <img src={movie.Poster} alt={movie.Title} />}

      <h2>Available Torrents</h2>
      {torrents.length === 0 && <p>No torrents found for this movie.</p>}
      <ul>
        {torrents.map((t, i) => {
          const filename = t.behaviorHints?.filename || t.title;
          return (
            <li key={i}>
              <p>{t.title || "Unnamed torrent"}</p>
              {t.infoHash && <p>Hash: {t.infoHash}</p>}
              <a
                href={`/stream?filename=${encodeURIComponent(filename)}&magnet=${encodeURIComponent(t.infoHash)}`}
                target="_blank"
                rel="noopener noreferrer"
              >
                Watch Movie (MP4 Stream)
              </a>
              <br />
              {/*<video
                width={640}
                height={360}
                controls
                src={`/stream?filename=${encodeURIComponent(filename)}&magnet=${encodeURIComponent(t.infoHash)}`}
              >
                Your browser does not support the video tag.
              </video>*/}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
