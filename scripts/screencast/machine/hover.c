/* Glides the pointer to x, y (in points) inside the machine, as a hand would
 * before clicking, so a video shows it travel rather than jump, and the app
 * under it has redrawn its hover by the time Sheprd Agent clicks (the
 * agent's own click moves and presses 8 ms apart, which a busy machine does
 * not redraw in). Runs through `sheprd machine exec`, so it acts with the
 * agent's Accessibility permission.
 *   hover X Y [MILLISECONDS]   (default 450; 0 jumps)
 *   cc -O2 -o hover hover.c -framework ApplicationServices */
#include <ApplicationServices/ApplicationServices.h>
#include <math.h>
#include <stdlib.h>
#include <unistd.h>

static void move_to(CGEventSourceRef source, CGPoint point) {
    CGEventRef move = CGEventCreateMouseEvent(source, kCGEventMouseMoved, point, kCGMouseButtonLeft);
    if (!move) return;
    CGEventPost(kCGHIDEventTap, move);
    CFRelease(move);
}

int main(int argc, char **argv) {
    if (argc < 3) return 2;
    CGPoint to = CGPointMake(atof(argv[1]), atof(argv[2]));
    int ms = argc > 3 ? atoi(argv[3]) : 450;
    CGEventSourceRef source = CGEventSourceCreate(kCGEventSourceStateHIDSystemState);
    CGEventRef now = CGEventCreate(NULL);
    CGPoint from = now ? CGEventGetLocation(now) : to;
    if (now) CFRelease(now);
    double distance = hypot(to.x - from.x, to.y - from.y);
    /* Short hops take less time, so small moves do not crawl. */
    if (distance < 400) ms = (int)(ms * (0.4 + 0.6 * distance / 400));
    int steps = ms / 8;
    for (int i = 1; i <= steps; i++) {
        double t = (double)i / steps;
        double eased = t < 0.5 ? 4 * t * t * t : 1 - pow(-2 * t + 2, 3) / 2; /* ease in-out */
        move_to(source, CGPointMake(from.x + (to.x - from.x) * eased, from.y + (to.y - from.y) * eased));
        usleep(8000);
    }
    move_to(source, to);
    if (source) CFRelease(source);
    return 0;
}
