#!/bin/sh

#SBATCH --job-name=bambam-gtfs
#SBATCH --nodes=1
#SBATCH --ntasks=100
#SBATCH --mem=240GB
#SBATCH --tmp=500GB
#SBATCH --mail-type=ALL

module load bzip2

/projects/mepcore/bin/bambam-gtfs \
    import \
    --input /home/$USER/data/bam/gtfs/2025-10-08 \
    --parallelism 50 \
    --output-directory "/projects/mepcore/data/out/rfitzger/2025-10-17-gtfs" \
    --vertices-compass-filename "/projects/mepcore/lib/routee-compass-tomtom/data/tomtom_national/vertices-complete.csv.gz" \
    --start-date 09-01-2023 \
    --end-date 09-01-2023 \
    --start-time 08:00:00 \
    --end-time 09:00:00 \
    --date-mapping-policy nearest-date-time-range \
    --date-mapping-date-tolerance 730 \
    --date-mapping-match-weekday true \