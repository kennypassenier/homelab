# Why the Proxmox host swaps, and what Kenny can do about it

*Kenny's question, form W3, 2026-09-02: "kan je uitzoeken waarom die exact
aan het swappen is en hoe ik dat kan tegengaan? maar niks zelf veranderen."*
**Nothing here was applied.** Everything below is measured and then left
alone, because two of the three levers touch VMs on the no-touch list.

## What is actually in the swap

Read from the running machine, 2026-09-02:

```
host RAM                     31811 MB
promised to guests           47360 MB   (35072 to containers, 12288 to VMs)
host swap                     8191 MB, of which 8178 in use
```

Of those 8178 MB, **5047 belong to two processes**:

| | allocated | resident | swapped |
|---|---|---|---|
| VM 100 · OPNsense | 4096 MB | 1988 MB | **1606 MB** |
| VM 101 · Home Assistant | 6144 MB | 2530 MB | **3441 MB** |

The rest is Proxmox's own daemons — `jsvc` 425 MB, `java` 319 MB,
`pvedaemon` 144 MB, `pvescheduler` 117 MB.

## Why those two, specifically

Three facts, and the third is the one that explains it.

**1. The machine is oversubscribed.** 47 GB promised on 31 GB. That is not
automatically wrong — guests rarely use their whole allocation — but it
means that when everything is touched at once, something must go out.

**2. A VM's memory is opaque to the host.** To Linux, a running VM is one
large anonymous mapping. The host cannot see which parts the guest cares
about, so under pressure it swaps whatever has been cold longest. A guest's
own page cache looks exactly like a guest's live data from outside.

**3. Ballooning is present but has nothing to give.** Both VMs run the
balloon device with `free-page-reporting=on`, so the mechanism designed for
this is enabled. But QEMU reports:

```
VM 100   balloon: actual=4096  max_mem=4096
VM 101   balloon: actual=6144  max_mem=6144  total_mem=5864  free_mem=650
                  mem_swapped_in=0  mem_swapped_out=0
```

`actual = max_mem` on both means neither has been asked to shrink and
neither has. And Home Assistant's guest reports only **650 MB free** of its
5864 MB — from inside, there is almost nothing spare to hand back, even
with free-page reporting on.

The last pair of numbers is the reassuring one: `mem_swapped_in=0` and
`mem_swapped_out=0` say the GUEST is not swapping at all. Home Assistant is
comfortable. The host is quietly carrying the cost for it.

## What Kenny can do, strongest lever first

**1. Reduce what is promised.** 47 GB on 31 GB is the root cause; every
other measure works around it. Home Assistant at 6144 MB with 2530 MB
actually resident is the obvious candidate — but see the caveat below.

**2. Set `balloon` BELOW `memory` on the two VMs.** With `balloon: 3072` on
a 6144 MB VM, the host can ask the guest to shrink when memory is tight, and
the GUEST then decides what to release — dropping its own cache first. That
is strictly better than the host guessing, which is what happens today.
Nothing in Proxmox does this by default.

**3. Find out what uses 5.2 GB inside Home Assistant.** That is a Home
Assistant question rather than a Proxmox one; the recorder database and its
page cache are the usual answer. Fixing it there is better than fighting it
from outside.

**4. `swapoff -a && swapon -a`** clears the ballast and forces those 5 GB
back into RAM. There is 17.9 GB available so it fits, but it is a spike, and
it touches both no-touch VMs at once. Low value: it does not change why the
pages went out, so they will go out again.

**Not recommended: lowering `vm.swappiness` further.** It is already 10.

## The caveat on shrinking Home Assistant

Its guest reports 650 MB free. Cutting its allocation without first finding
out what fills it would move the pressure INSIDE the VM, where the guest
would start swapping — and unlike the host, Home Assistant going slow is
something the house notices. Measure inside first, then resize.

## What was changed on 2026-09-02 and what was not

**Changed:** thirteen managed containers now run with `swap_mb: 0` (D95).
That is a different problem with a different cause — page cache filling a
container's own limit — and it does not affect anything above.

**Not changed:** the host's swap partition, both VMs, `vm.swappiness`, and
every allocation. Kenny asked for the investigation only.
