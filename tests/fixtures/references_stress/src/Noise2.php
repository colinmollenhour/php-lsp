<?php

namespace App;

class Noise2
{
    private function compute(): int
    {
        return 2;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
