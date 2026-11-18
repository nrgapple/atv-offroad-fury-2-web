# Optional Sport and Simulation handling

Local riders select a preset under **Settings → Game Settings → Physics**. Original uses the existing ATV and motorcycle solvers. Sport eases power during excessive wheelspin. Simulation gives the rider finer responsibility for throttle and braking. A new ride or race restart supplies fresh components. Repositioning during an event carries its wear forward. Online rooms use Original and modified runs use the existing record exclusion.

## Contact and balance

The optional solver integrates each wheel's angular speed from its drive torque, braking, bearing resistance and the reaction torque from the ground. It measures slip against the velocity of the support at the contact point. A spinning wheel keeps its momentum in flight and exchanges it with the chassis on landing. Both components of tire force share a load-dependent friction limit. Wheel dimensions, inertia, gearing and terrain properties come from the existing model data. The relaxation time (20 ms) and grip multipliers are game tuning. [PhysX wheel dynamics](https://nvidia-omniverse.github.io/PhysX/physx/latest/_api_build/classPxVehicleDirectDrivetrainComponent.html)

The ATV applies the resulting forces at four suspension contacts. The motorcycle blends the rider's steering command with a bank angle measured from the resultant support force. The balance target is bounded by the bike's own roll limit. Its ground balance controller recedes at low speeds and during donuts.

The bike rider stands taller with speed, absorbs suspension compression and moves rearward under braking. One critically damped body displacement drives both the animated pelvis and a 32% share of the combined center of mass. The enhanced two-bone IK solves directly to the hand and boot contact markers. It limits pelvis travel to the reach of the limbs currently pinned to the bike. A slide or stop releases the boots over an eighth of a second. The ATV uses a smaller bracing movement, weighted by speed, yaw and braking, in both the riding clips and chassis mass distribution. Full manual ATV lean takes priority. These offsets and response times are authored game tuning.

In Sport and Simulation, the bike's rear contact supplies the position and scrub speed for dirt emission. Coasting raises less dirt than a sliding or spinning tire, including wheelspin at low vehicle speed. The existing surface filters and particle budget still apply.

## Wear and heat

Sliding work consumes tread independently at each tire. Spring resistance rises over the final 30% of travel. Fast compression in the final quarter of travel wears that corner's damper and reduces damping by up to 22%. Brake heat accumulates from torque and angular travel. Moving air increases cooling and hot brakes lose some braking torque. Brake rotors darken and brighten with the simulated work at their own wheel. The color ramps up early so repeated braking is visible from the riding camera. Heat is normalized to the gameplay fade threshold. The temperature and service life of a particular vehicle would need measured rotor, pad, tire and damper data. [Brembo on heat and off-road brake performance](https://www.brembo.com/en/motorsport/powersport)
