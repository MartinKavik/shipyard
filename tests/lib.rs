mod book;
mod borrow;
mod iteration;
#[cfg(feature = "serde")]
mod serde;
mod window;
mod workload;

use shipyard::error;
#[cfg(feature = "parallel")]
use shipyard::iterators;
use shipyard::*;

#[test]
fn run() {
    let world = World::new();
    world.run(
        |(mut entities, mut usizes, mut u32s): (EntitiesViewMut, ViewMut<usize>, ViewMut<u32>)| {
            entities.add_entity((&mut usizes, &mut u32s), (0usize, 1u32));
            entities.add_entity((&mut usizes, &mut u32s), (2usize, 3u32));

            // possible to borrow twice as immutable
            let mut iter1 = (&usizes).iter();
            let _iter2 = (&usizes).iter();
            assert_eq!(iter1.next(), Some(&0));

            // impossible to borrow twice as mutable
            // if switched, the next two lines should trigger an shipyard::error
            let _iter = (&mut usizes).iter();
            let mut iter = (&mut usizes).iter();
            assert_eq!(iter.next(), Some(&mut 0));
            assert_eq!(iter.next(), Some(&mut 2));
            assert_eq!(iter.next(), None);

            // possible to borrow twice as immutable
            let mut iter = (&usizes, &u32s).iter();
            let _iter = (&usizes, &u32s).iter();
            assert_eq!(iter.next(), Some((&0, &1)));
            assert_eq!(iter.next(), Some((&2, &3)));
            assert_eq!(iter.next(), None);

            // impossible to borrow twice as mutable
            // if switched, the next two lines should trigger an shipyard::error
            let _iter = (&mut usizes, &u32s).iter();
            let mut iter = (&mut usizes, &u32s).iter();
            assert_eq!(iter.next(), Some((&mut 0, &1)));
            assert_eq!(iter.next(), Some((&mut 2, &3)));
            assert_eq!(iter.next(), None);
        },
    );
}

#[cfg(feature = "parallel")]
#[cfg_attr(miri, ignore)]
#[test]
fn thread_pool() {
    let world = World::new();
    world.run(|thread_pool: ThreadPoolView| {
        use rayon::prelude::*;

        let vec = vec![0, 1, 2, 3];
        thread_pool.install(|| {
            assert_eq!(vec.into_par_iter().sum::<i32>(), 6);
        });
    })
}

#[test]
fn system() {
    fn system1((mut usizes, u32s): (ViewMut<usize>, View<u32>)) {
        (&mut usizes, &u32s).iter().for_each(|(x, y)| {
            *x += *y as usize;
        });
    }

    let world = World::new();

    world.run(
        |(mut entities, mut usizes, mut u32s): (EntitiesViewMut, ViewMut<usize>, ViewMut<u32>)| {
            entities.add_entity((&mut usizes, &mut u32s), (0usize, 1u32));
            entities.add_entity((&mut usizes, &mut u32s), (2usize, 3u32));
        },
    );

    world.add_workload("").with_system(system!(system1)).build();
    world.run_default();
    world.run(|usizes: View<usize>| {
        let mut iter = usizes.iter();
        assert_eq!(iter.next(), Some(&1));
        assert_eq!(iter.next(), Some(&5));
        assert_eq!(iter.next(), None);
    });
}

#[test]
fn systems() {
    fn system1((mut usizes, u32s): (ViewMut<usize>, View<u32>)) {
        (&mut usizes, &u32s).iter().for_each(|(x, y)| {
            *x += *y as usize;
        });
    }

    fn system2(mut usizes: ViewMut<usize>) {
        (&mut usizes,).iter().for_each(|x| {
            *x += 1;
        });
    }

    let world = World::new();

    world.run(
        |(mut entities, mut usizes, mut u32s): (EntitiesViewMut, ViewMut<usize>, ViewMut<u32>)| {
            entities.add_entity((&mut usizes, &mut u32s), (0usize, 1u32));
            entities.add_entity((&mut usizes, &mut u32s), (2usize, 3u32));
        },
    );

    world
        .add_workload("")
        .with_system(system!(system1))
        .with_system(system!(system2))
        .build();
    world.run_default();
    world.run(|usizes: View<usize>| {
        let mut iter = usizes.iter();
        assert_eq!(iter.next(), Some(&2));
        assert_eq!(iter.next(), Some(&6));
        assert_eq!(iter.next(), None);
    });
}

#[cfg(feature = "parallel")]
#[cfg_attr(miri, ignore)]
#[test]
fn simple_parallel_sum() {
    use rayon::prelude::*;

    let world = World::new();

    world.run(
        |(mut entities, mut usizes, mut u32s): (EntitiesViewMut, ViewMut<usize>, ViewMut<u32>)| {
            entities.add_entity((&mut usizes, &mut u32s), (1usize, 2u32));
            entities.add_entity((&mut usizes, &mut u32s), (3usize, 4u32));
        },
    );

    world.run(|(usizes, thread_pool): (ViewMut<usize>, ThreadPoolView)| {
        thread_pool.install(|| {
            let sum: usize = (&usizes,).par_iter().cloned().sum();
            assert_eq!(sum, 4);
        });
    });
}

#[cfg(feature = "parallel")]
#[cfg_attr(miri, ignore)]
#[test]
fn tight_parallel_iterator() {
    use iterators::ParIter2;
    use rayon::prelude::*;

    let world = World::new();

    world.run(
        |(mut entities, mut usizes, mut u32s): (EntitiesViewMut, ViewMut<usize>, ViewMut<u32>)| {
            (&mut usizes, &mut u32s).tight_pack();
            entities.add_entity((&mut usizes, &mut u32s), (0usize, 1u32));
            entities.add_entity((&mut usizes, &mut u32s), (2usize, 3u32));
        },
    );

    world.run(
        |(mut usizes, u32s, thread_pool): (ViewMut<usize>, View<u32>, ThreadPoolView)| {
            let counter = std::sync::atomic::AtomicUsize::new(0);
            thread_pool.install(|| {
                if let ParIter2::Tight(iter) = (&mut usizes, &u32s).par_iter() {
                    iter.for_each(|(x, y)| {
                        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        *x += *y as usize;
                    });
                } else {
                    panic!()
                }
            });
            assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 2);
            let mut iter = (&mut usizes).iter();
            assert_eq!(iter.next(), Some(&mut 1));
            assert_eq!(iter.next(), Some(&mut 5));
            assert_eq!(iter.next(), None);
        },
    );
}

#[cfg(feature = "parallel")]
#[cfg_attr(miri, ignore)]
#[test]
fn parallel_iterator() {
    use rayon::prelude::*;

    let world = World::new();

    world.run(
        |(mut entities, mut usizes, mut u32s): (EntitiesViewMut, ViewMut<usize>, ViewMut<u32>)| {
            entities.add_entity((&mut usizes, &mut u32s), (0usize, 1u32));
            entities.add_entity((&mut usizes, &mut u32s), (2usize, 3u32));
        },
    );

    world.run(
        |(mut usizes, u32s, thread_pool): (ViewMut<usize>, View<u32>, ThreadPoolView)| {
            let counter = std::sync::atomic::AtomicUsize::new(0);
            thread_pool.install(|| {
                (&mut usizes, &u32s).par_iter().for_each(|(x, y)| {
                    counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    *x += *y as usize;
                });
            });
            assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 2);
            let mut iter = (&mut usizes).iter();
            assert_eq!(iter.next(), Some(&mut 1));
            assert_eq!(iter.next(), Some(&mut 5));
            assert_eq!(iter.next(), None);
        },
    );
}

#[cfg(feature = "parallel")]
#[cfg_attr(miri, ignore)]
#[test]
fn loose_parallel_iterator() {
    use iterators::ParIter2;
    use rayon::prelude::*;

    let world = World::new();

    world.run(
        |(mut entities, mut usizes, mut u32s): (EntitiesViewMut, ViewMut<usize>, ViewMut<u32>)| {
            LoosePack::<(usize,)>::loose_pack((&mut usizes, &mut u32s));
            entities.add_entity((&mut usizes, &mut u32s), (0usize, 1u32));
            entities.add_entity((&mut usizes, &mut u32s), (2usize, 3u32));
        },
    );

    world.run(
        |(mut usizes, u32s, thread_pool): (ViewMut<usize>, View<u32>, ThreadPoolView)| {
            let counter = std::sync::atomic::AtomicUsize::new(0);
            thread_pool.install(|| {
                if let ParIter2::Loose(iter) = (&mut usizes, &u32s).par_iter() {
                    iter.for_each(|(x, y)| {
                        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        *x += *y as usize;
                    });
                } else {
                    panic!()
                }
            });
            assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 2);
            let mut iter = (&mut usizes).iter();
            assert_eq!(iter.next(), Some(&mut 1));
            assert_eq!(iter.next(), Some(&mut 5));
            assert_eq!(iter.next(), None);
        },
    );
}

#[cfg(feature = "parallel")]
#[cfg_attr(miri, ignore)]
#[test]
fn two_workloads() {
    fn system1(_: View<usize>) {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    let world = World::new();
    world.add_workload("").with_system(system!(system1)).build();

    rayon::scope(|s| {
        s.spawn(|_| world.run_default());
        s.spawn(|_| world.run_default());
    });
}

#[cfg(feature = "parallel")]
#[cfg_attr(miri, ignore)]
#[test]
#[should_panic(
    expected = "called `Result::unwrap()` on an `Err` value: System lib::two_bad_workloads::system1 failed: Cannot mutably borrow usize storage while it\'s already borrowed."
)]
fn two_bad_workloads() {
    fn system1(_: ViewMut<usize>) {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    let world = World::new();
    world.add_workload("").with_system(system!(system1)).build();

    rayon::scope(|s| {
        s.spawn(|_| world.run_default());
        s.spawn(|_| world.run_default());
    });
}

#[test]
fn add_component_with_old_key() {
    let world = World::new();

    let entity = {
        let (mut entities, mut usizes, mut u32s) =
            world.borrow::<(EntitiesViewMut, ViewMut<usize>, ViewMut<u32>)>();
        entities.add_entity((&mut usizes, &mut u32s), (0usize, 1u32))
    };

    world.run(|mut all_storages: AllStoragesViewMut| {
        all_storages.delete(entity);
    });

    let (entities, mut usizes, mut u32s) =
        world.borrow::<(EntitiesViewMut, ViewMut<usize>, ViewMut<u32>)>();
    assert_eq!(
        entities.try_add_component((&mut usizes, &mut u32s), (1, 2), entity),
        Err(error::AddComponent::EntityIsNotAlive)
    );
}

#[cfg(feature = "parallel")]
#[cfg_attr(miri, ignore)]
#[test]
fn par_update_pack() {
    use rayon::prelude::*;

    let world = World::new();

    world.run(
        |(mut entities, mut usizes): (EntitiesViewMut, ViewMut<usize>)| {
            usizes.update_pack();
            entities.add_entity(&mut usizes, 0);
            entities.add_entity(&mut usizes, 1);
            entities.add_entity(&mut usizes, 2);
            entities.add_entity(&mut usizes, 3);

            usizes.clear_inserted();

            (&usizes).par_iter().sum::<usize>();

            assert_eq!(usizes.modified().len(), 0);

            (&mut usizes).par_iter().for_each(|i| {
                *i += 1;
            });

            let mut iter = usizes.inserted().iter();
            assert_eq!(iter.next(), None);

            let mut iter = usizes.modified_mut().iter();
            assert_eq!(iter.next(), Some(&mut 1));
            assert_eq!(iter.next(), Some(&mut 2));
            assert_eq!(iter.next(), Some(&mut 3));
            assert_eq!(iter.next(), Some(&mut 4));
            assert_eq!(iter.next(), None);
        },
    );
}

#[cfg(feature = "parallel")]
#[cfg_attr(miri, ignore)]
#[test]
fn par_multiple_update_pack() {
    use iterators::ParIter2;
    use rayon::prelude::*;

    let world = World::new();

    world.run(
        |(mut entities, mut usizes, mut u32s): (EntitiesViewMut, ViewMut<usize>, ViewMut<u32>)| {
            u32s.update_pack();
            entities.add_entity((&mut usizes, &mut u32s), (0usize, 1u32));
            entities.add_entity(&mut usizes, 2usize);
            entities.add_entity((&mut usizes, &mut u32s), (4usize, 5u32));
            entities.add_entity(&mut u32s, 7u32);
            entities.add_entity((&mut usizes, &mut u32s), (8usize, 9u32));
            entities.add_entity((&mut usizes,), (10usize,));

            u32s.clear_inserted();
        },
    );

    world.run(|(mut usizes, mut u32s): (ViewMut<usize>, ViewMut<u32>)| {
        if let ParIter2::NonPacked(iter) = (&usizes, &u32s).par_iter() {
            iter.for_each(|_| {});
        } else {
            panic!("not packed");
        }

        assert_eq!(u32s.modified().len(), 0);

        if let ParIter2::NonPacked(iter) = (&mut usizes, &u32s).par_iter() {
            iter.for_each(|_| {});
        } else {
            panic!("not packed");
        }

        assert_eq!(u32s.modified().len(), 0);

        if let ParIter2::NonPacked(iter) = (&usizes, &mut u32s).par_iter() {
            iter.for_each(|_| {});
        } else {
            panic!("not packed");
        }

        let mut modified: Vec<_> = u32s.modified().iter().collect();
        modified.sort_unstable();
        assert_eq!(modified, vec![&1, &5, &7, &9]);

        let mut iter: Vec<_> = (&u32s).iter().collect();
        iter.sort_unstable();
        assert_eq!(iter, vec![&1, &5, &7, &9]);
    });
}

#[cfg(feature = "parallel")]
#[cfg_attr(miri, ignore)]
#[test]
fn par_update_filter() {
    use rayon::prelude::*;

    let world = World::new();

    world.run(
        |(mut entities, mut usizes): (EntitiesViewMut, ViewMut<usize>)| {
            usizes.update_pack();
            entities.add_entity(&mut usizes, 0);
            entities.add_entity(&mut usizes, 1);
            entities.add_entity(&mut usizes, 2);
            entities.add_entity(&mut usizes, 3);

            usizes.clear_inserted();

            (&mut usizes)
                .par_iter()
                .filter(|x| **x % 2 == 0)
                .for_each(|i| {
                    *i += 1;
                });

            let mut iter = usizes.inserted().iter();
            assert_eq!(iter.next(), None);

            let mut modified: Vec<_> = usizes.modified().iter().collect();
            modified.sort_unstable();
            assert_eq!(modified, vec![&1, &1, &3, &3]);

            let mut iter: Vec<_> = (&usizes).iter().collect();
            iter.sort_unstable();
            assert_eq!(iter, vec![&1, &1, &3, &3]);
        },
    );
}
