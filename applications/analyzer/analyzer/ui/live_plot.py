"""Live matplotlib plotting for real-time kinetic runs.

Opens a matplotlib figure with one subplot per active channel and
updates it once per second from a thread-safe deque. Intended for
kinetic assays (ethanol, other enzyme assays) where watching the
trace come up is useful.
"""

from __future__ import annotations

import logging
from collections import deque
from dataclasses import dataclass
from typing import Iterable


log = logging.getLogger(__name__)


@dataclass
class LivePlot:
    """Live plot for one or more channels.

    Usage::

        plot = LivePlot(channel_names=("340nm",), title="Ethanol assay")
        plot.start()
        # ... from the hot loop ...
        plot.push("340nm", t_rel, A340)
        # ... at the end ...
        plot.save("run.png")
        plot.stop()
    """

    channel_names: tuple[str, ...]
    title: str = "Analyzer live plot"
    max_points: int = 10_000
    refresh_hz: float = 1.0

    def __post_init__(self) -> None:
        self._t: dict[str, deque[float]] = {
            name: deque(maxlen=self.max_points) for name in self.channel_names
        }
        self._y: dict[str, deque[float]] = {
            name: deque(maxlen=self.max_points) for name in self.channel_names
        }
        self._fig = None
        self._axes = None
        self._lines: dict[str, object] = {}
        self._animation = None
        self._running = False

    def start(self) -> None:
        try:
            import matplotlib.pyplot as plt
            from matplotlib.animation import FuncAnimation
        except ImportError as exc:
            raise ImportError("matplotlib is required for LivePlot") from exc

        n = len(self.channel_names)
        self._fig, axes = plt.subplots(n, 1, figsize=(9, 2.5 * n), sharex=True)
        if n == 1:
            axes = [axes]
        self._axes = axes

        for ax, name in zip(axes, self.channel_names):
            (line,) = ax.plot([], [], lw=1.2)
            self._lines[name] = line
            ax.set_ylabel(f"{name}\n(raw / OD)")
            ax.grid(True, alpha=0.3)
        axes[-1].set_xlabel("Time (s)")
        self._fig.suptitle(self.title)

        self._animation = FuncAnimation(
            self._fig,
            self._update,
            interval=int(1000 / self.refresh_hz),
            blit=False,
            cache_frame_data=False,
        )
        self._running = True
        plt.show(block=False)

    def push(self, channel_name: str, t: float, y: float) -> None:
        if channel_name not in self._t:
            log.warning("Unknown channel for live plot: %s", channel_name)
            return
        self._t[channel_name].append(t)
        self._y[channel_name].append(y)

    def _update(self, _frame: int) -> Iterable[object]:
        for name, line in self._lines.items():
            line.set_data(list(self._t[name]), list(self._y[name]))
        for ax, name in zip(self._axes, self.channel_names):
            if self._t[name]:
                ax.set_xlim(min(self._t[name]), max(self._t[name]) + 1)
                ax.relim()
                ax.autoscale_view()
        return list(self._lines.values())

    def save(self, path: str) -> None:
        if self._fig is not None:
            self._fig.savefig(path, dpi=150, bbox_inches="tight")

    def stop(self) -> None:
        self._running = False
        if self._animation is not None:
            self._animation.event_source.stop()
