export default function VideoPlayer({ imdbId }: { imdbId: string }) {
  return (
    <div className="w-full max-w-3xl mx-auto">
      <video
        className="w-full rounded-lg shadow-lg"
        src={`http://localhost:8080/stream/${imdbId}.mp4`}
        controls
      />
    </div>
  );
}
