<?php
namespace App;

class Beta {
    public function __construct(
        private Alpha $alpha,
    ) {}

    public function run(): string {
        return $this->alpha->describe();
    }
}
