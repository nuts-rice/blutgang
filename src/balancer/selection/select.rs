use crate::Rpc;
use std::time::SystemTime;

// Generic entry point fn to select the next rpc and return its position
pub fn pick(list: &mut [Rpc]) -> (Rpc, Option<usize>) {
    // If len is 1, return the only element
    if list.len() == 1 {
        return (list[0].clone(), Some(0));
    } else if list.is_empty() {
        return (Rpc::default(), None);
    }

    algo(list)
}

// Sorting algo
pub fn argsort(data: &[Rpc]) -> Vec<usize> {
    let mut indices = (0..data.len()).collect::<Vec<usize>>();

    // Use sort_by_cached_key with a closure that compares latency
    // Uses pdqsort and does not allocate so should be fast
    indices.sort_unstable_by_key(|&index| data[index].status.latency as u64);

    indices
}

// Selection algorithms
//
// Selected via features. selection-weighed-round-robin is a default feature.
// In order to have custom algos, you must add and enable the feature,
// as well as modify the cfg of the default algo to accomodate your new feature.
//
#[cfg(all(
    feature = "selection-weighed-round-robin",
    not(feature = "selection-random"),
    not(feature = "old-weighted-round-robin"),
))]
fn algo(list: &mut [Rpc]) -> (Rpc, Option<usize>) {
    // Sort by latency
    let indices = argsort(list);

    let time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("Failed to get current time")
        .as_micros();

    // Picks the second fastest one rpc that meets our requirements
    // Also take into account min_delta_time

    // Set fastest rpc as default
    let mut choice = indices[0];
    let mut choice_consecutive = 0;
    for i in indices.iter().rev() {
        if list[*i].max_consecutive > list[*i].consecutive
            && (time - list[*i].last_used > list[*i].min_time_delta)
        {
            choice = *i;
            choice_consecutive = list[*i].consecutive;
        }

        // remove consecutive
        list[*i].consecutive = 0;
    }

    // If no RPC has been selected, fall back to the fastest RPC
    list[choice].consecutive = choice_consecutive + 1;
    list[choice].last_used = time;
    (list[choice].clone(), Some(choice))
}

#[cfg(all(
    feature = "selection-weighed-round-robin",
    feature = "selection-random"
))]
fn algo(list: &mut [Rpc]) -> (Rpc, Option<usize>) {
    use rand::Rng;

    let mut rng = rand::thread_rng();
    let index = rng.gen_range(0..list.len());
    (list[index].clone(), Some(index))
}

#[cfg(all(
    feature = "selection-weighed-round-robin",
    feature = "old-weighted-round-robin",
))]
fn algo(list: &mut [Rpc]) -> (Rpc, Option<usize>) {
    // Sort by latency
    let indices = argsort(list);

    // Picks the second fastest one if the fastest one has maxed out
    if list[indices[0]].max_consecutive <= list[indices[0]].consecutive {
        list[indices[1]].consecutive = 1;
        list[indices[0]].consecutive = 0;
        return (list[indices[1]].clone(), Some(indices[1]));
    }

    list[indices[0]].consecutive += 1;
    (list[indices[0]].clone(), Some(indices[0]))
}

// Tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sort_algo() {
        let mut rpc1 = Rpc::default();
        let mut rpc2 = Rpc::default();
        let mut rpc3 = Rpc::default();

        rpc1.status.latency = 1.0;
        rpc2.status.latency = 2.0;
        rpc3.status.latency = 3.0;

        let v = vec![rpc2, rpc3, rpc1];
        let vx = v.clone();
        let i = argsort(&v);
        assert_eq!(i, &[2, 0, 1]);
        assert_eq!(v[0].get_url(), vx[0].get_url());
    }

    // Test picking the fastest RPC
    // Change the latencies of the other ones to simulate
    // real network fluctuations.
    #[test]
    fn test_pick() {
        let mut rpc1 = Rpc::default();
        let mut rpc2 = Rpc::default();
        let mut rpc3 = Rpc::default();

        rpc1.status.latency = 3.0;
        rpc1.max_consecutive = 10;
        rpc1.min_time_delta = 100;

        rpc2.status.latency = 7.0;
        rpc2.max_consecutive = 10;
        rpc2.min_time_delta = 100;

        rpc3.status.latency = 5.0;
        rpc3.max_consecutive = 10;
        rpc3.min_time_delta = 100;

        let mut rpc_list = vec![rpc1, rpc2, rpc3];

        let (rpc, index) = pick(&mut rpc_list);
        println!("rpc: {:?}", rpc);
        assert_eq!(rpc.status.latency, 3.0);
        assert_eq!(index, Some(0));

        rpc_list[0].status.latency = 10000.0;

        let (rpc, index) = pick(&mut rpc_list);
        println!("rpc index: {:?}", index);
        assert_eq!(rpc.status.latency, 5.0);
        assert_eq!(index, Some(2));

        rpc_list[2].status.latency = 100000.0;

        let (rpc, index) = pick(&mut rpc_list);
        assert_eq!(rpc.status.latency, 7.0);
        assert_eq!(index, Some(1));
    }

    // Test max_delay when picking rpcs
    #[test]
    fn test_pick_max_delay() {
        let mut rpc1 = Rpc::default();
        let mut rpc2 = Rpc::default();
        let mut rpc3 = Rpc::default();

        rpc1.status.latency = 3.0;
        rpc1.max_consecutive = 10;
        rpc1.min_time_delta = 1701357164371770;
        rpc1.last_used = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("Failed to get current time")
            .as_micros();

        rpc2.status.latency = 7.0;
        rpc2.max_consecutive = 10;
        rpc2.min_time_delta = 1;

        rpc3.status.latency = 5.0;
        rpc3.max_consecutive = 10;
        rpc3.min_time_delta = 10000000;

        let mut rpc_list = vec![rpc1, rpc2, rpc3];

        // Pick rpc3 becauese rpc1 does not meet last used requirements
        let (rpc, index) = pick(&mut rpc_list);
        println!("rpc: {:?}", rpc);
        assert_eq!(rpc.status.latency, 5.0);
        assert_eq!(index, Some(2));

        // pick rpc2 because rpc3 was just used
        let (rpc, index) = pick(&mut rpc_list);
        println!("rpc index: {:?}", index);
        assert_eq!(rpc.status.latency, 7.0);
        assert_eq!(index, Some(1));
    }
}
