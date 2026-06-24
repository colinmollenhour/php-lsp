<?php

namespace App;

class Noise7
{
    private function compute(): int
    {
        return 7;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
