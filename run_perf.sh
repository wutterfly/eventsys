#! /bin/bash

cargo b -r && cargo b -r

# set permissions
echo -1 | sudo tee /proc/sys/kernel/perf_event_paranoid > /dev/null
echo 0 | sudo tee /proc/sys/kernel/kptr_restrict > /dev/null

# record performence counter
perf record -e cycles,branch-misses,branch-load-misses,cache-misses --call-graph dwarf -- target/release/eventsys
#perf record --call-graph dwarf -- target/release/eventsys 

echo "Finished recording"

# remove old data
if test -f "perf.data.old"; then
    echo "Removing old data"
    rm perf.data.old
fi

# convert to flamegraph
perf script | inferno-collapse-perf | inferno-flamegraph > profile.svg

perf stat -ad -r 100 -e cycles,branches,branch-misses,branch-loads,branch-load-misses,cache-misses,dTLB-loads,dTLB-load-misses target/release/eventsys
#perf stat -ad -r 100 target/release/eventsys